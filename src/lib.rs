//! A risc-v kernel.

#![feature(custom_test_frameworks)]
#![no_main]
#![no_std]
#![reexport_test_harness_main = "kernel_test"]
#![test_runner(test::runner)]
#![warn(
    clippy::all,
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_core,
    missing_docs,
    rustdoc::all
)]

pub mod kalloc;
pub mod kheap;
pub mod mem;
pub mod paging;
pub mod proc;
pub mod riscv;
pub mod test;
pub mod uart;

use crate::paging::MappingError;

#[cfg(test)]
boot!(test_main);

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn test_main() {
    let cpuid = unsafe { proc::cpuid() };
    if cpuid == 0 {
        kernel_test();
    }
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
#[unsafe(no_mangle)]
const extern "C" fn eh_personality() {}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    test::panic_handler(info)
}

/// Kernel error.
#[derive(Debug)]
pub enum Error {
    /// The kernel and/or its subsystems are in an invalid state.
    BadMapping(MappingError),

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

#[macro_export]
macro_rules! boot {
    ($path:path) => {
        #[unsafe(export_name = "boot")]
        extern "C" fn __impl_start() {
            use core::arch::asm;

            use $crate::{
                mem::{BSS_ADDR, STACK_ADDR},
                riscv::{
                    r_menvcfg, r_mhartid, r_mstatus, r_sie, w_medeleg, w_menvcfg, w_mepc,
                    w_mideleg, w_mstatus, w_pmpaddr0, w_pmpcfg0, w_satp, w_sie, w_tp,
                },
            };

            // validate the signature of the program entry point
            let _: extern "C" fn() = $path;
            unsafe {
                // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
                w_mstatus({
                    let mut mstatus = r_mstatus();
                    mstatus &= !(0b11 << 11);
                    mstatus |= 0b01 << 11;
                    mstatus
                });
                // set `mepc` to the address of `main`, so the cpu jumps to `main` after `mret` is called
                w_mepc($path as *const () as usize);
                // set `satp` to 0 to disable paging
                w_satp(0);
                // delegate all exceptions and interrupts to supervisor mode
                w_medeleg(0xffff);
                w_mideleg(0xffff);
                // set `sie` to enable specific interrupts:
                // 1 << 9: supervisor external interrupt enable bit
                // 1 << 5: supervisor timer interrupt enable bit
                w_sie(r_sie() | (1 << 9) | (1 << 5));
                // give supervisor mode access to all physical memory
                w_pmpaddr0(0x3f_ffff_ffff_ffff);
                w_pmpcfg0(0xf);
                // enable hardware updates of page table entries' a and d bits
                w_menvcfg(r_menvcfg() | (1 << 61));
                let hartid = r_mhartid();
                if hartid == 0 {
                    // initialize the bss memory section to 0
                    // only one cpu is responsible for writing, and there always exists a cpu with id 0
                    let ptr = BSS_ADDR as *mut u8;
                    let len = STACK_ADDR - BSS_ADDR;
                    core::slice::from_raw_parts_mut(ptr, len).fill(0);
                }
                // set the thread pointer to the current cpu id
                w_tp(hartid);
                // switch to supervisor mode and jump to `main`
                asm!("mret");
            }
        }
    };
}
