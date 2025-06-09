use core::{cell::{RefCell, RefMut, UnsafeCell}, ops::{Deref, DerefMut}};
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
