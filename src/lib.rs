//! A risc-v kernel.

#![no_std]
#![warn(
    clippy::all,
    clippy::missing_safety_doc,
    clippy::nursery,
    clippy::pedantic,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    rust_2021_compatibility,
    rust_2024_compatibility,
    rustdoc::all
)]

mod addr;
mod heap;
mod kalloc;
mod paging;
mod proc;
mod riscv;
mod uart;

use core::{
    arch::asm,
    slice,
    sync::atomic::{self, AtomicBool},
};

use crate::{
    proc::cpuid,
    riscv::{
        r_menvcfg, r_mhartid, r_mstatus, r_sie, w_medeleg, w_menvcfg, w_mepc, w_mideleg, w_mstatus,
        w_pmpaddr0, w_pmpcfg0, w_satp, w_sie, w_tp,
    },
};

/// The size of a page in bytes.
const PAGE_SIZE: usize = 4096;

/// Address of the 16550 UART device on the `virt` machine in `QEMU`
const UART_ADDR: usize = 0x1000_0000;

unsafe extern "C" {
    /// Address of the physical memory.
    static MEM_ADDR: usize;

    /// Size of the physical memory.
    static MEM_SIZE: usize;

    /// Memory address of the `.tramp` section.
    static TRAMP_ADDR: usize;

    /// Memory address of the `.rodata` section.
    static RODATA_ADDR: usize;

    /// Memory address of the `.data` section.
    static DATA_ADDR: usize;

    /// Memory address of the `.bss` section.
    static BSS_ADDR: usize;

    /// Memory address of the kernel's stack.
    static STACK_ADDR: usize;

    /// Memory address of the kernel's heap.
    static HEAP_ADDR: usize;

}

/// Errors that occur when working with the page table.
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

// TODO: initialize the timer
#[unsafe(no_mangle)]
extern "C" fn boot() {
    unsafe {
        // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
        w_mstatus({
            let mut mstatus = r_mstatus();
            mstatus &= !(0b11 << 11);
            mstatus |= 0b01 << 11;
            mstatus
        });
        // set `mepc` to the address of `main`, so the cpu jumps to `main` after `mret` is called
        w_mepc(main as *const () as usize);
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

extern "C" fn main() {
    static INIT: AtomicBool = AtomicBool::new(false);
    if cpuid() == 0 {
        println!();
        println!("osmium kernel is booting");
        println!();
        // physical page allocator
        kalloc::init();
        // kernel page table
        paging::init();
        // enable paging
        paging::inithart();
        // global rust allocator
        heap::init();
        // finish initialization
        INIT.store(true, atomic::Ordering::Release);
    } else {
        // wait for cpu 0 to finish initialization
        while !INIT.load(atomic::Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // enable paging
        paging::inithart();
    }
    println!("cpu#{} started", cpuid());
    loop {
        core::hint::spin_loop();
    }
}

/// The lang item `eh_personality` is a function used by the failure mechanisms of the compiler.
///
/// This is often mapped to GCC’s personality function (see the std implementation for more
/// information), but programs which don’t trigger a panic can be assured that this function is
/// never called. Additionally, a `eh_catch_typeinfo` static is needed for certain targets which
/// implement Rust panics on top of C++ exceptions.
#[unsafe(no_mangle)]
const extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("aborting!");
    if let Some(p) = info.location() {
        println!("panic: {} ({}:{})", info.message(), p.file(), p.line());
    } else {
        println!("panic: no information available");
    }
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
