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

pub mod kalloc;
pub mod kheap;
pub mod mem;
pub mod paging;
pub mod proc;
pub mod spinlock;
#[cfg(test)]
mod test;
pub mod trap;
pub mod uart;

use core::{
    arch::asm,
    sync::atomic::{self, AtomicBool},
};

use mem::{BSS_ADDR, STACK_ADDR};

#[cfg(test)]
main!(test_main);

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn test_main() {
    kinit();
    let cpuid = unsafe { proc::cpuid() };
    if cpuid == 0 {
        kernel_test();
    }
}

#[unsafe(no_mangle)]
const extern "C" fn eh_personality() {}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    test::panic(info)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("aborting!");
    if let Some(p) = info.location() {
        println!("panic: {} ({}:{})", info.message(), p.file(), p.line());
    } else {
        println!("panic: no information available");
    }
    abort()
}

/// Kernel error.
#[derive(Debug)]
pub enum Error {
    /// The kernel and/or its subsystems are in an invalid state.
    BadMapping(paging::MappingError),

    /// The kernel and/or its subsystems reach an unexpected state.
    BadState,

    /// There's no memory left on the device for the kernel.
    OutOfMemory,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMapping(err) => write!(f, "{err}"),
            Self::BadState => write!(f, "bad state"),
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

/// Create a function with the unmangled name "minit".
///
/// This function will be called by the entrypoint in `boot.S` in machine mode to configure the
/// system. Once configured, all CPUs switch into supervisor mode and jump the address of the
/// function that was given to this macro.
#[macro_export]
macro_rules! main {
    ($path:path) => {
        #[unsafe(export_name = "minit")]
        extern "C" fn __impl_start() {
            // validate the signature of the program entry point
            let _: extern "C" fn() = $path;
            // setup the system in machine mode before switching to supervisor mode and jumps to the
            // function located at `$path`
            $crate::minit($path as *const () as usize);
        }
    };
}

/// Configure the system in machine mode, switch into supervisor mode, and jump to the address given
/// to `mepc`.
pub fn minit(mepc: usize) {
    unsafe {
        // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
        let mut mstatus: usize;
        asm!("csrr {}, mstatus", out(reg) mstatus);
        mstatus &= !(0b11 << 11);
        mstatus |= 0b01 << 11;
        asm!("csrw mstatus, {}", in(reg) mstatus);

        // set `mepc` to the address of $path, so the cpu jumps to $path after `mret` is called
        asm!("csrw mepc, {}", in(reg) mepc);

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

        // get the id of the currently executing hardware thread
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

/// Initializes the kernel and its subsystems.
pub fn kinit() {
    static INIT: AtomicBool = AtomicBool::new(false);
    let cpuid = unsafe { proc::cpuid() };
    if cpuid == 0 {
        println!();
        println!("osmium kernel is booting");
        println!();
        // page allocator
        kalloc::init();
        // kernel page table
        paging::kvminit();
        // enable paging
        paging::kvminithart();
        // object allocator
        kheap::init();
        // finish initialization
        INIT.store(true, atomic::Ordering::Release);
    } else {
        // wait for cpu 0 to finish initialization
        while !INIT.load(atomic::Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // enable paging
        paging::kvminithart();
    }
    println!("cpu#{cpuid} started");
}
