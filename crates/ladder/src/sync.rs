use std::sync::{Mutex, MutexGuard, PoisonError};

/// A panicking game thread must not stop the ladder: the guarded state stays consistent between calls.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
