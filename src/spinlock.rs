//! Spinlock implementations for RISC-V 64-bit systems.

use core::{
    cell::{Cell, UnsafeCell},
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{self, AtomicBool},
};

use crate::proc::{PushOff, cpuid};

/// A spin-based lock providing mutually exclusive access to data.
pub struct Spinlock<T: ?Sized> {
    locked: AtomicBool,
    cpu: Cell<usize>,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}
unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}

unsafe impl<T: ?Sized> Sync for SpinlockGuard<'_, T> where for<'a> &'a mut T: Sync {}
unsafe impl<T: ?Sized> Send for SpinlockGuard<'_, T> where for<'a> &'a mut T: Send {}

impl<T> Spinlock<T> {
    /// Creates a new spinlock protecting the given `data`.
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            cpu: Cell::new(0),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> Spinlock<T> {
    /// Returns whether the current CPU is holding this lock.
    pub fn holding(&self) -> bool {
        let cpuid = unsafe { cpuid() };
        self.locked.load(atomic::Ordering::Relaxed) && self.cpu.get() == cpuid
    }

    /// Acquires the lock if it has not been acquired. Otherwise, blocks the current CPU until the
    /// lock can be acquired.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let mut _push_off = PushOff::new();
        if self.holding() {
            panic!("spinlock - reentrance")
        }
        loop {
            match self.try_lock(_push_off) {
                Ok(guard) => break guard,
                Err(push_off) => {
                    _push_off = push_off;
                }
            }
            while self.locked.load(atomic::Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
    }

    fn try_lock(&self, _push_off: PushOff) -> Result<SpinlockGuard<'_, T>, PushOff> {
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
                _push_off,
            })
        } else {
            Err(_push_off)
        }
    }
}

/// A guard protecting access to data behinds a [`SpinLock`]
pub struct SpinlockGuard<'l, T: ?Sized + 'l> {
    lock: &'l Spinlock<T>,
    _push_off: PushOff,
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
