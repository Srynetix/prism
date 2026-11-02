//! The passes of a collector the window can ask to read again now ("Refresh"), and wait for: the
//! request returns at the end of the first pass that started after it, with that pass's error.

use prism_base::tr;
use anyhow::{Context, Result, bail};
use std::time::Duration;
use tokio::sync::{Notify, watch};

#[derive(Clone, Debug, Default, PartialEq)]
struct State {
    /// Finished passes.
    done: u64,
    /// A pass is under way: it read the source before a request made now, its end does not answer it.
    busy: bool,
    /// The last finished pass's error.
    error: Option<String>,
}

pub struct Passes {
    state: watch::Sender<State>,
    wake: Notify,
}

impl Default for Passes {
    fn default() -> Self {
        Self { state: watch::channel(State::default()).0, wake: Notify::new() }
    }
}

impl Passes {
    /// The collector's wait between two passes ends on a request: `select!` it with the pause.
    pub async fn requested(&self) {
        self.wake.notified().await
    }

    /// A pass starts now.
    pub fn begin(&self) {
        self.state.send_if_modified(|s| {
            s.busy = true;
            false
        });
    }

    /// The pass ends, with its error if any. Ends the pass and counts it in one step: a request
    /// never sees the pass done while still counted as busy, nor the other way round.
    pub fn end(&self, error: Option<String>) {
        self.state.send_modify(|s| {
            s.done += 1;
            s.busy = false;
            s.error = error;
        });
    }

    /// Wakes the collector for a pass, without waiting for it.
    pub fn poke(&self) {
        self.wake.notify_one();
    }

    /// Asks for a pass and waits for it, `within` at most; `what` names the source in the errors.
    pub async fn request(&self, within: Duration, what: &str) -> Result<()> {
        let mut rx = self.state.subscribe();
        let wanted = {
            let s = rx.borrow();
            s.done + if s.busy { 2 } else { 1 }
        };
        self.poke();
        let pass = tokio::time::timeout(within, rx.wait_for(|s| s.done >= wanted))
            .await
            .with_context(|| tr!("{what} refresh timed out", what = what))?
            .with_context(|| tr!("{what} reader stopped", what = what))?;
        match &pass.error {
            Some(e) => bail!("{e}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A collector whose passes take `pass` and fail with their number when it is odd.
    fn collector(passes: Arc<Passes>, pass: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut n = 0u64;
            loop {
                passes.requested().await;
                passes.begin();
                tokio::time::sleep(pass).await;
                n += 1;
                passes.end((n % 2 == 1).then(|| format!("pass {n}")));
            }
        })
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_waits_for_the_next_pass_and_its_error() {
        let passes = Arc::new(Passes::default());
        let run = collector(passes.clone(), Duration::from_secs(1));
        assert_eq!(passes.request(Duration::from_secs(10), "X").await.unwrap_err().to_string(), "pass 1");
        assert!(passes.request(Duration::from_secs(10), "X").await.is_ok());
        run.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_during_a_pass_waits_for_the_one_after() {
        let passes = Arc::new(Passes::default());
        let run = collector(passes.clone(), Duration::from_secs(5));
        passes.poke();
        // Pass 1 is under way: it read the source before the request.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let start = tokio::time::Instant::now();
        // Pass 2 answers, without error: pass 1's would have been "pass 1".
        assert!(passes.request(Duration::from_secs(30), "X").await.is_ok());
        assert_eq!(start.elapsed(), Duration::from_secs(9));
        run.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_without_a_collector_times_out() {
        let passes = Passes::default();
        let err = passes.request(Duration::from_secs(3), "Jira").await.unwrap_err();
        assert_eq!(err.to_string(), "Jira refresh timed out");
    }
}
