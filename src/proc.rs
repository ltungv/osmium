//! Process management.

use crate::riscv::registers::{
    sstatus::{self, Sstatus},
    tp,
};

/// Total number of CPUs in the system.
pub const NCPU: usize = 4;

/// Per CPU metadata.
pub struct Cpu {
    push_offs: usize,
    intr_enabled: bool,
}

impl Cpu {
    const fn zero() -> Self {
        Cpu {
            push_offs: 0,
            intr_enabled: false,
        }
    }

    /// Get a mutable reference to metadata of this CPU.
    pub fn current() -> &'static mut Self {
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
/// When the first [`PushOff`] is created, all interrupts are disable until the last [`PushOff`]
/// falls out of scope.
pub struct PushOff;

impl Default for PushOff {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PushOff {
    fn drop(&mut self) {
        let sstatus = unsafe { sstatus::read() };
        if sstatus.has(Sstatus::SIE) {
            panic!("pop_intr_disable - interruptible");
        }
        let cpu = Cpu::current();
        cpu.push_offs -= 1;
        if cpu.push_offs == 0 && cpu.intr_enabled {
            unsafe {
                sstatus::set(Sstatus::SIE);
            }
        }
    }
}

impl PushOff {
    /// Creates a new [`PushOff`], ensuring that the curent CPU has all interrupts disabled for the
    /// lifetime of the [`PushOff`] instance.
    pub fn new() -> Self {
        let cpu = Cpu::current();
        if cpu.push_offs == 0 {
            let sstatus = unsafe { sstatus::read_clear(Sstatus::SIE) };
            cpu.intr_enabled = sstatus.has(Sstatus::SIE);
        }
        cpu.push_offs += 1;
        Self
    }
}
