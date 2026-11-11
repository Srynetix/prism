//! The system clipboard, used from one thread of its own, never the main one. On Linux, the main
//! thread blocked on the clipboard cannot answer for the window when the text was copied from it
//! (its webview): the app froze (the clipboard plugin warns of it).

use std::{sync::LazyLock, time::Duration};

use prism_base::tr;
use prism_commands::commands::Cmd;

/// How long the clipboard may take to answer before it is reported unavailable.
pub const WAIT: Duration = Duration::from_secs(5);

type Job = Box<dyn FnOnce() + Send>;

/// A thread that runs the clipboard's calls one after another: in the order they were asked (two
/// copies in a row land in their order), and a call that never returns holds that thread alone,
/// not one more for each call after it (nor the plugin's lock from many).
pub struct Worker(std::sync::mpsc::Sender<Job>);

impl Worker {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new().name("clipboard".into()).spawn(move || {
            for job in rx {
                job();
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("clipboard: no thread of its own ({e})");
        }
        Self(tx)
    }

    /// Runs `f` on its thread, `wait` at most: past it, an error rather than a promise that
    /// never returns. A call whose caller gave up before its turn does not run (a copy told
    /// failed does not land later); one under way when it gave up still ends.
    pub async fn run<T: Send + 'static, E: std::fmt::Display + Send + 'static>(&self, wait: Duration, f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Cmd<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            if !tx.is_closed() {
                let _ = tx.send(f());
            }
        });
        if self.0.send(job).is_err() {
            return Err(tr!("The clipboard is unavailable").into());
        }
        match tokio::time::timeout(wait, rx).await {
            Ok(Ok(done)) => done.map_err(|e| e.to_string().into()),
            Ok(Err(_)) => Err(tr!("The clipboard is unavailable").into()),
            Err(_) => Err(tr!("The clipboard did not answer within {seconds} s", seconds = wait.as_secs().max(1)).into()),
        }
    }
}

static WORKER: LazyLock<Worker> = LazyLock::new(Worker::new);

/// Runs `f` on the clipboard's thread, `WAIT` at most.
pub async fn run<T: Send + 'static, E: std::fmt::Display + Send + 'static>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Cmd<T> {
    WORKER.run(WAIT, f).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    /// A clipboard that does not answer: an error past the wait; the calls after it wait their
    /// turn, those given up never run, and it answers again once the stuck one ends.
    #[tokio::test]
    async fn a_clipboard_that_does_not_answer_holds_its_thread_alone() {
        let worker = Worker::new();
        let wait = Duration::from_millis(50);
        let started = std::time::Instant::now();
        let stuck = worker.run(wait, || {
            std::thread::sleep(Duration::from_millis(600));
            Ok::<_, String>("late")
        });
        let err = format!("{:?}", stuck.await.unwrap_err());
        assert!(err.contains("did not answer") || err.contains("pas répondu"), "{err}");
        // A wide margin: told at the wait (50 ms), not at its end (600 ms), even under load.
        assert!(started.elapsed() < Duration::from_millis(500), "told at the wait, not at its end");
        // Asked while it is stuck, given up: never run.
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let given_up = worker.run(wait, move || {
            flag.store(true, Ordering::SeqCst);
            Ok::<_, String>(())
        });
        assert!(given_up.await.is_err());
        // Its turn after the stuck one: answers.
        assert_eq!(worker.run(Duration::from_secs(5), || Ok::<_, String>("text")).await.unwrap(), "text");
        assert!(!ran.load(Ordering::SeqCst), "a call given up does not run later");
        assert!(format!("{:?}", worker.run(Duration::from_secs(5), || Err::<(), _>("no text")).await.unwrap_err()).contains("no text"));
    }

    /// Two copies in a row land in their order.
    #[tokio::test]
    async fn its_calls_run_in_their_order() {
        let worker = Worker::new();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let calls = (0..5).map(|i| {
            let seen = seen.clone();
            worker.run(Duration::from_secs(5), move || {
                std::thread::sleep(Duration::from_millis(5 - i));
                seen.lock().unwrap().push(i);
                Ok::<_, String>(())
            })
        });
        for c in futures::future::join_all(calls).await {
            c.unwrap();
        }
        assert_eq!(*seen.lock().unwrap(), [0, 1, 2, 3, 4]);
    }
}
