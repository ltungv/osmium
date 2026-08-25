//! A risc-v kernel.

#![cfg_attr(test, allow(unused))]
#![feature(custom_test_frameworks)]
#![no_std]
#![reexport_test_harness_main = "kernel_test"]
#![warn(
    clippy::all,
    clippy::alloc_instead_of_core,
    // clippy::missing_safety_doc,
    clippy::std_instead_of_core,
    // clippy::undocumented_unsafe_blocks,
    missing_docs,
    rustdoc::all
)]

pub mod kalloc;
pub mod kheap;
pub mod mem;
pub mod paging;
pub mod proc;
pub mod riscv;
pub mod uart;

/// The size of a page in bytes.
pub const PAGE_SIZE: usize = 4096;

/// Address of the UART device on the `virt` machine in `QEMU`
pub const UART_ADDR: usize = 0x1000_0000;

unsafe extern "C" {
    /// Address of the physical memory.
    pub static MEM_ADDR: usize;

    /// Size of the physical memory.
    pub static MEM_SIZE: usize;

    /// Address of the `.tramp` section.
    pub static TRAMP_ADDR: usize;

    /// Address of the `.rodata` section.
    pub static RODATA_ADDR: usize;

    /// Address of the `.data` section.
    pub static DATA_ADDR: usize;

    /// Address of the `.bss` section.
    pub static BSS_ADDR: usize;

    /// Address of the kernel's stack.
    pub static STACK_ADDR: usize;

    /// Address of the kernel's heap.
    pub static HEAP_ADDR: usize;
}

/// Kernel error.
#[derive(Debug)]
enum Error {
    /// The kernel and/or its subsystems are in an invalid state.
    InvalidState,

    /// There's no memory left on the device for the kernel.
    OutOfMemory,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidState => write!(f, "invalid state"),
            Self::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn boot() {
    use core::{arch::asm, slice};

    use crate::riscv::{
        r_menvcfg, r_mhartid, r_mstatus, r_sie, w_medeleg, w_menvcfg, w_mepc, w_mideleg, w_mstatus,
        w_pmpaddr0, w_pmpcfg0, w_satp, w_sie, w_tp,
    };

    unsafe {
        // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
        w_mstatus({
            let mut mstatus = r_mstatus();
            mstatus &= !(0b11 << 11);
            mstatus |= 0b01 << 11;
            mstatus
        });
        // set `mepc` to the address of `main`, so the cpu jumps to `main` after `mret` is called
        w_mepc(kernel_test as *const () as usize);
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
            let bss = slice::from_raw_parts_mut(BSS_ADDR as *mut u8, STACK_ADDR - BSS_ADDR);
            bss.fill(0);
        }
        // set the thread pointer to the current cpu id
        w_tp(hartid);
        // switch to supervisor mode and jump to `main`
        asm!("mret");
    }
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    println!("running {} tests", tests.len());
    for test in tests {
        test();
    }
}
