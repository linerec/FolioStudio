//! Scoped worker threads with dynamic work distribution.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Threads to use for `jobs` independent jobs.
pub fn workers(jobs: usize) -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16).min(jobs.max(1))
}

/// Runs `job(&mut state, i)` for `0..n` on scoped worker threads; each worker builds its own
/// `state` with `init`. Results come back in index order. A panicking job propagates out
/// of the scope; `read_package` runs on its own thread and reports that as an error.
pub fn parallel<S, T: Send>(n: usize, init: impl Fn() -> S + Sync, job: impl Fn(&mut S, usize) -> T + Sync) -> Vec<T> {
    if n <= 1 {
        let mut state = init();
        return (0..n).map(|i| job(&mut state, i)).collect();
    }
    let next = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<T>>> = Mutex::new((0..n).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers(n) {
            scope.spawn(|| {
                let mut state = init();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = job(&mut state, i);
                    if let Ok(mut s) = slots.lock() {
                        s[i] = Some(r);
                    }
                }
            });
        }
    });
    slots.into_inner().unwrap_or_default().into_iter().flatten().collect()
}
