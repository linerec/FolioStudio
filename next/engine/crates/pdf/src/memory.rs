use std::sync::{Condvar, Mutex};

/// Limits concurrent temporary allocations. A job larger than the budget runs alone.
pub(crate) struct Budget {
    capacity: u64,
    available: Mutex<u64>,
    ready: Condvar,
}

impl Budget {
    pub(crate) fn new(capacity: u64) -> Self {
        Self { capacity, available: Mutex::new(capacity), ready: Condvar::new() }
    }

    pub(crate) fn run<T>(&self, bytes: u64, job: impl FnOnce() -> T) -> T {
        let bytes = bytes.min(self.capacity);
        let mut available = self.available.lock().expect("PDF memory budget lock");
        while *available < bytes {
            available = self.ready.wait(available).expect("PDF memory budget lock");
        }
        *available -= bytes;
        drop(available);
        let _permit = Permit { budget: self, bytes };
        job()
    }
}

struct Permit<'a> {
    budget: &'a Budget,
    bytes: u64,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.budget.available.lock().expect("PDF memory budget lock") += self.bytes;
        self.budget.ready.notify_all();
    }
}
