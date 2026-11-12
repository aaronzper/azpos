use core::ops::{Deref, DerefMut};
use spin::{Mutex, MutexGuard};

use crate::interrupts::{disable_interrupts, enable_interrupts, interrupts_enabled};

/// A special "mutex" that disables interrupts when acquired. Wraps around a
/// spinlock so if multiple cores try to get this only one does at a time.
pub struct KIntMutex<T> {
    inner: Mutex<T>,
}

impl<T> KIntMutex<T> {
    pub const fn new(value: T) -> KIntMutex<T> {
        KIntMutex {
            inner: Mutex::new(value)
        }
    }

    fn get_disable_guard(&self) -> Option<IntDisabledGuard> {
        if interrupts_enabled() {
            disable_interrupts();
            Some(IntDisabledGuard)
        } else {
            None
        }
    }

    /// Disables interrupts (if they're enabled) and tries to acquire the lock.
    /// Re-enables them if it fails to do so.
    pub fn try_lock<'a>(&'a self) -> Option<KIntMutexGuard<'a, T>> {
        let disable_guard = self.get_disable_guard();

        let mut_guard = self.inner.try_lock()?;
        Some(KIntMutexGuard {
            inner: mut_guard,
            disable_guard
        })
    }

    /// Disables interrupts and spins until it can acquire the lock
    pub fn lock<'a>(&'a self) -> KIntMutexGuard<'a, T> {
        let disable_guard = self.get_disable_guard();

        KIntMutexGuard {
            inner: self.inner.lock(),
            disable_guard,
        }
    }
}

pub struct KIntMutexGuard<'a, T> {
    inner: MutexGuard<'a, T>,
    disable_guard: Option<IntDisabledGuard>,
}

struct IntDisabledGuard;
impl Drop for IntDisabledGuard {
    fn drop(&mut self) {
        enable_interrupts();
    }
}

impl<'a, T> Deref for KIntMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for KIntMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;

    ktest! {
        /// Interrupts are off while the guard lives and on after.
        fn guard_disables_interrupts_until_drop() {
            let mutex = KIntMutex::new(5);
            let guard = mutex.lock();
            let during = interrupts_enabled();
            let value = *guard;
            drop(guard);
            check!(!during);
            check_eq!(value, 5);
            check!(interrupts_enabled());
        }

        /// The drop of the inner guard keeps interrupts off.
        fn inner_guard_drop_keeps_interrupts_off() {
            let outer = KIntMutex::new(());
            let inner = KIntMutex::new(());
            let outer_guard = outer.lock();
            let inner_guard = inner.lock();
            drop(inner_guard);
            let after_inner = interrupts_enabled();
            drop(outer_guard);
            check!(!after_inner);
            check!(interrupts_enabled());
        }

        /// `try_lock` of a held lock gives `None` and keeps interrupts on.
        fn try_lock_of_a_held_lock_keeps_interrupts_on() {
            let mutex = KIntMutex::new(());
            // The spin lock holds the mutex with interrupts on.
            let held = mutex.inner.try_lock().or_fail("inner try_lock")?;
            let result = mutex.try_lock().is_none();
            let after = interrupts_enabled();
            drop(held);
            check!(result);
            check!(after);
        }

        /// `try_lock` of a held lock gives `None` and keeps interrupts off.
        fn try_lock_of_a_held_lock_keeps_interrupts_off() {
            let mutex = KIntMutex::new(());
            let guard = mutex.lock();
            let result = mutex.try_lock().is_none();
            let after = interrupts_enabled();
            drop(guard);
            check!(result);
            check!(!after);
        }

        /// A guard that starts with interrupts off does not turn them on.
        fn guard_keeps_interrupts_off_if_they_were_off() {
            let mutex = KIntMutex::new(());
            disable_interrupts();
            drop(mutex.lock());
            let after = interrupts_enabled();
            enable_interrupts();
            check!(!after);
        }
    }
}
