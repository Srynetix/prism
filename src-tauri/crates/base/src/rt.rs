//! The async runtime the app runs on (Tauri's): what the library crates spawn their collectors on,
//! without depending on Tauri.
//!
//! The collectors started from one reading of the config form a `Generation`: what they spawn, and
//! what that spawns in turn, belongs to it, so that a change of the config stops them all at once
//! before starting them again from the new one (no restart of Prism).

use std::{
    cell::RefCell,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
};

use tokio::{
    runtime::Handle,
    task::{AbortHandle, JoinHandle},
};

static HANDLE: OnceLock<Handle> = OnceLock::new();

/// The app's runtime, given once at launch, before anything is spawned.
pub fn init(handle: Handle) {
    let _ = HANDLE.set(handle);
}

/// Spawns `future` on the app's runtime; without one (tests), on the current one. Spawned from a
/// collector (`Generation::current`), it belongs to that collector's generation.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let Some(generation) = Generation::current() else { return spawn_on_app(future) };
    let handle = spawn_on_app(CURRENT.scope(generation.clone(), future));
    generation.track(handle.abort_handle());
    handle
}

fn spawn_on_app<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match HANDLE.get() {
        Some(handle) => handle.spawn(future),
        None => tokio::spawn(future),
    }
}

tokio::task_local! {
    static CURRENT: Generation;
}

thread_local! {
    /// The generation being started on this thread (`Generation::enter`), outside any task.
    static ENTERED: RefCell<Option<Generation>> = const { RefCell::new(None) };
}

/// The collectors started from one reading of the config, and everything they spawned.
#[derive(Clone, Default)]
pub struct Generation(Arc<Inner>);

#[derive(Default)]
struct Inner {
    tasks: Mutex<Vec<AbortHandle>>,
    cancelled: AtomicBool,
}

impl Generation {
    /// The generation of the collector running here: the task's, or the one being started on this
    /// thread; none outside the collectors (commands, agents, the window).
    pub fn current() -> Option<Self> {
        CURRENT.try_with(Clone::clone).ok().or_else(|| ENTERED.with(|g| g.borrow().clone()))
    }

    /// Runs `start`, what it spawns (`spawn`) belonging to this generation.
    pub fn enter<R>(&self, start: impl FnOnce() -> R) -> R {
        let previous = ENTERED.with(|g| g.replace(Some(self.clone())));
        let out = start();
        ENTERED.with(|g| *g.borrow_mut() = previous);
        out
    }

    /// Runs `start` outside any generation started on this thread: what it spawns runs on for good
    /// (a block that does not reload).
    pub fn outside<R>(start: impl FnOnce() -> R) -> R {
        let previous = ENTERED.with(|g| g.take());
        let out = start();
        ENTERED.with(|g| *g.borrow_mut() = previous);
        out
    }

    fn track(&self, task: AbortHandle) {
        if self.cancelled() {
            task.abort();
            return;
        }
        let mut tasks = self.0.tasks.lock().expect("generation tasks");
        // Collectors spawn as they go (a pass, a probe): those done are dropped now and then.
        if tasks.len() >= 64 {
            tasks.retain(|t| !t.is_finished());
        }
        tasks.push(task);
    }

    /// Stops all its tasks (at their next `.await`); its threads see it with `cancelled`.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
        for task in self.0.tasks.lock().expect("generation tasks").drain(..) {
            task.abort();
        }
    }

    /// Stopped: a thread of its own (the macOS Calendar's) ends there.
    pub fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }
}

/// A value the collectors read for the rest of the program, set again when the config changes:
/// each one set is kept for good (leaked), so that readers hold `&'static`. A handful per change
/// of the config, which is rare.
pub struct Swap<T: 'static>(RwLock<Option<&'static T>>);

impl<T: Send + Sync + 'static> Swap<T> {
    pub const fn new() -> Self {
        Self(RwLock::new(None))
    }

    pub fn set(&self, value: T) -> &'static T {
        let value: &'static T = Box::leak(Box::new(value));
        *self.0.write().expect("swap") = Some(value);
        value
    }

    pub fn get(&self) -> Option<&'static T> {
        *self.0.read().expect("swap")
    }
}

impl<T: Send + Sync + 'static> Default for Swap<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_stops_what_the_generation_spawned_and_what_that_spawned() {
        let generation = Generation::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let inner = tx.clone();
        let outer = generation.enter(|| {
            spawn(async move {
                // Spawned from within: the same generation.
                spawn(async move {
                    loop {
                        let _ = inner.send(());
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                });
                std::future::pending::<()>().await
            })
        });
        rx.recv().await.unwrap();
        generation.cancel();
        assert!(outer.await.unwrap_err().is_cancelled());
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        while rx.try_recv().is_ok() {}
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(rx.try_recv().is_err(), "the inner task still runs");
        drop(tx);
    }

    #[tokio::test]
    async fn outside_a_generation_nothing_is_tracked() {
        assert!(Generation::current().is_none());
        let task = spawn(async { Generation::current().is_none() });
        assert!(task.await.unwrap());
    }

    #[test]
    fn swap_replaces_its_value() {
        static S: Swap<u32> = Swap::new();
        assert!(S.get().is_none());
        S.set(1);
        let one = S.get().unwrap();
        S.set(2);
        assert_eq!((*one, *S.get().unwrap()), (1, 2));
    }
}
