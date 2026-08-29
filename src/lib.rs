//! A risc-v kernel.

#![feature(custom_test_frameworks)]
#![no_main]
#![no_std]
#![reexport_test_harness_main = "kernel_test"]
#![test_runner(test::run)]
#![warn(
    clippy::all,
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_core,
    missing_docs,
    rustdoc::all
)]

use core::arch::asm;

pub mod kalloc;
pub mod kheap;
pub mod mem;
pub mod paging;
pub mod proc;
pub mod uart;

#[cfg(test)]
mod test;

#[cfg(test)]
boot!(test_main);

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn test_main() {
    let cpuid = unsafe { proc::cpuid() };
    if cpuid == 0 {
        kernel_test();
    }
}

#[cfg(test)]
#[unsafe(no_mangle)]
const extern "C" fn eh_personality() {}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    test::panic(info)
}

/// Kernel error.
#[derive(Debug)]
pub enum Error {
    /// The kernel and/or its subsystems are in an invalid state.
    BadMapping(paging::MappingError),

    /// There's no memory left on the device for the kernel.
    OutOfMemory,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMapping(err) => write!(f, "{err}"),
            Self::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

/// Abort execution, preventing the current CPU from execution.
pub fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
        core::hint::spin_loop();
    }
}

/// Creates a function that will be called by the entrypoint in `boot.S`.
///
/// This macro creates a function that will be executed in machine mode to configure the system.
/// After the configuration is done, the system will switch into supervisor mode and jump the
/// address of the function given to this macro.
#[macro_export]
macro_rules! boot {
    ($path:path) => {
        #[unsafe(export_name = "boot")]
        extern "C" fn __impl_start() {
            use core::arch::asm;

            use $crate::mem::{BSS_ADDR, STACK_ADDR};

            // validate the signature of the program entry point
            let _: extern "C" fn() = $path;
            unsafe {
                // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
                let mut mstatus: usize;
                asm!("csrr {}, mstatus", out(reg) mstatus);
                mstatus &= !(0b11 << 11);
                mstatus |= 0b01 << 11;
                asm!("csrw mstatus, {}", in(reg) mstatus);

                // set `mepc` to the address of $path, so the cpu jumps to $path after `mret` is called
                asm!("csrw mepc, {}", in(reg) $path as *const () as usize);

                // set `satp` to 0 to disable paging
                asm!("csrw satp, {}", in(reg) 0);

                // delegate all exceptions and interrupts to supervisor mode
                asm!("csrw medeleg, {}", in(reg) 0xffff);
                asm!("csrw mideleg, {}", in(reg) 0xffff);

                // set `sie` to enable specific interrupts:
                // 1 << 9: supervisor external interrupt enable bit
                // 1 << 5: supervisor timer interrupt enable bit
                let sie: usize;
                asm!("csrr {}, sie", out(reg) sie);
                asm!("csrw sie, {}", in(reg) sie | (1 << 9) | (1 << 5));

                // give supervisor mode access to all physical memory
                asm!("csrw pmpaddr0, {}", in(reg) 0x3f_ffff_ffff_ffffu64);
                asm!("csrw pmpcfg0, {}", in(reg) 0xf);

                // enable hardware updates of page table entries' a and d bits
                let menvcfg: usize;
                asm!("csrr {}, menvcfg", out(reg) menvcfg);
                asm!("csrw menvcfg, {}", in(reg) menvcfg | (1 << 61));

                // get the id of the currently executing hardware thread.
                let hartid: usize;
                asm!("csrr {}, mhartid", out(reg) hartid);

                // initialize the bss memory section to 0
                // only one cpu is responsible for writing, and there always exists a cpu with id 0
                if hartid == 0 {
                    let ptr = BSS_ADDR as *mut u8;
                    let len = STACK_ADDR - BSS_ADDR;
                    core::slice::from_raw_parts_mut(ptr, len).fill(0);
                }

                // set the thread pointer to the current cpu id
                asm!("mv tp, {}", in(reg) hartid);

                // switch to supervisor mode and jump to `main`
                asm!("mret");
            }
        }
    };
}
