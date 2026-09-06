//! Spinlock implementations for RISC-V 64-bit systems.

use core::{
    cell::{Cell, UnsafeCell},
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{self, AtomicBool},
};

use crate::proc::{IntrDisable, cpuid};

/// A spin-based lock providing mutually exclusive access to data.
pub struct Spinlock<T: ?Sized> {
    locked: AtomicBool,
    cpu: Cell<usize>,
    name: &'static str,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}
unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}

unsafe impl<T: ?Sized> Sync for SpinlockGuard<'_, T> where for<'a> &'a mut T: Sync {}
unsafe impl<T: ?Sized> Send for SpinlockGuard<'_, T> where for<'a> &'a mut T: Send {}

impl<T> Spinlock<T> {
    /// Creates a new spinlock protecting the given `data`.
    pub const fn new(name: &'static str, data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            cpu: Cell::new(0),
            name,
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> Spinlock<T> {
    /// Returns whether the current CPU is holding this lock.
    pub fn holding(&self) -> bool {
        let _intr_disable = IntrDisable::new();
        let cpuid = unsafe { cpuid() };
        self.locked.load(atomic::Ordering::Relaxed) && self.cpu.get() == cpuid
    }

    /// Acquires the lock if it has not been acquired. Otherwise, blocks the current CPU until the
    /// lock can be acquired.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let mut _intr_disable = IntrDisable::new();
        if self.holding() {
            panic!(
                "spinlock ({}) - reentrance on cpu#{}",
                self.name,
                self.cpu.get()
            );
        }
        loop {
            match self.try_lock(_intr_disable) {
                Ok(guard) => break guard,
                Err(intr_disable) => {
                    _intr_disable = intr_disable;
                }
            }
            while self.locked.load(atomic::Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
    }

    fn try_lock(&self, _intr_disable: IntrDisable) -> Result<SpinlockGuard<'_, T>, IntrDisable> {
        if self
            .locked
            .compare_exchange_weak(
                false,
                true,
                atomic::Ordering::Acquire,
                atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            self.cpu.set(unsafe { cpuid() });
            Ok(SpinlockGuard {
                lock: self,
                _intr_disable,
            })
        } else {
            Err(_intr_disable)
        }
    }
}

/// A guard protecting access to data behinds a [`SpinLock`]
pub struct SpinlockGuard<'l, T: ?Sized + 'l> {
    lock: &'l Spinlock<T>,
    _intr_disable: IntrDisable,
}

impl<'l, T: ?Sized> Drop for SpinlockGuard<'l, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, atomic::Ordering::Release);
    }
}

impl<'l, T: ?Sized + fmt::Debug> fmt::Debug for SpinlockGuard<'l, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'l, T: ?Sized + fmt::Display> fmt::Display for SpinlockGuard<'l, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'l, T: ?Sized> Deref for SpinlockGuard<'l, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'l, T: ?Sized> DerefMut for SpinlockGuard<'l, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
