//! Process management.

use core::marker::PhantomData;

use crate::riscv::{
    sstatus::{self, Sstatus},
    tp,
};

/// Total number of CPUs in the system.
pub const NCPU: usize = 4;

/// Per CPU metadata.
pub struct Cpu {
    intr: bool,
    pins: usize,
}

impl Cpu {
    const fn zero() -> Self {
        Cpu {
            intr: false,
            pins: 0,
        }
    }

    /// Get a mutable reference to metadata of this CPU.
    ///
    /// # Safety
    ///
    /// See [`cpuid`] for details.
    pub unsafe fn current() -> &'static mut Self {
        static mut CPUS: [Cpu; 4] = [const { Cpu::zero() }; 4];
        unsafe { &mut CPUS[cpuid()] }
    }
}

/// Get the hardware thread id of the currently executing thread.
///
/// # Safety
///
/// Interrupt must be disable before calling this function to avoid racing with another thread on
/// the `tp` register when a context switch occurs.
pub unsafe fn cpuid() -> usize {
    unsafe { tp::read() }
}

/// The lifecycle of this struct determines a section of the program's execution where interrupts
/// are disable.
///
/// Under scenarios where a spinlock is used by both a thread and an interupts handler, the thread
/// and the interrupt handler can deadlock when an interrupt happens after the thread acquired the
/// spinlock. To avoid this, the kernel disables interrupt on a CPU when it acquires any lock.
///
/// When the first [`CpuPin`] is created, all interrupts are disable until the last [`CpuPin`]
/// falls out of scope.
pub struct CpuPin {
    _data: PhantomData<*mut ()>,
}

impl Default for CpuPin {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CpuPin {
    fn drop(&mut self) {
        let sstatus = unsafe { sstatus::read() };
        if sstatus.has(Sstatus::SIE) {
            panic!("cpu_pin - interruptible");
        }
        let cpu = unsafe { Cpu::current() };
        cpu.pins -= 1;
        if cpu.pins == 0 && cpu.intr {
            unsafe {
                sstatus::set(Sstatus::SIE);
            }
        }
    }
}

impl CpuPin {
    /// Creates a new [`CpuPin`], ensuring that the curent CPU has all interrupts disabled for the
    /// lifetime of the [`CpuPin`] instance.
    pub fn new() -> Self {
        let sstatus = unsafe { sstatus::read_clear(Sstatus::SIE) };
        let cpu = unsafe { Cpu::current() };
        if cpu.pins == 0 {
            cpu.intr = sstatus.has(Sstatus::SIE);
        }
        cpu.pins += 1;
        Self { _data: PhantomData }
    }
}
