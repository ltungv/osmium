//! A risc-v kernel.

#![cfg_attr(test, allow(unused))]
#![feature(custom_test_frameworks)]
#![no_std]
#![reexport_test_harness_main = "kernel_test"]
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
pub mod uart;

use crate::paging::MappingError;

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

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn boot() {
    kernel_test()
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    println!("running {} tests", tests.len());
    for test in tests {
        test();
    }
}
