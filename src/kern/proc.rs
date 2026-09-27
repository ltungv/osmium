//! Process management.

use core::marker::PhantomData;

use crate::rv::{
    sstatus::{self, Sstatus},
    tp,
};

static mut CPUS: [Cpu; 4] = [const { Cpu::zero() }; 4];

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
    pub fn current(pin: &mut CpuPin) -> &mut Self {
        unsafe { &mut CPUS[pin.cpuid()] }
    }
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
    // Making this struct `!Send + !Sync`.
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
        let cpu = Cpu::current(self);
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
        let mut pin = Self { _data: PhantomData };
        let cpu = Cpu::current(&mut pin);
        // Take note of the SIE bit if we're creating the first pin of this CPU.
        if cpu.pins == 0 {
            cpu.intr = sstatus.has(Sstatus::SIE);
        }
        cpu.pins += 1;
        pin
    }

    pub fn cpuid(&self) -> usize {
        unsafe { tp::read() }
    }
}
