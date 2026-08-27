//! A risc-v kernel.

#![no_main]
#![no_std]
#![warn(
    clippy::all,
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_core,
    missing_docs,
    rustdoc::all
)]

use core::{
    arch::asm,
    slice,
    sync::atomic::{self, AtomicBool},
};

use osmium::{
    BSS_ADDR, STACK_ADDR, kheap, paging, println, proc,
    riscv::{
        r_menvcfg, r_mhartid, r_mstatus, r_sie, w_medeleg, w_menvcfg, w_mepc, w_mideleg, w_mstatus,
        w_pmpaddr0, w_pmpcfg0, w_satp, w_sie, w_tp,
    },
};

/// The assembly entry point for the kernel.
///
/// This function sets up the basic CPU state (e.g., privilege mode, interrupts, memory protection,
/// and paging) before jumping to the main Rust entry point (`main`).
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

/// The main Rust entry point of the kernel.
///
/// This function is called by the `boot` assembly code. It routes execution to the
/// test runner if tests are enabled, or to `kernel_main` for normal operation.
extern "C" fn main() {
    static INIT: AtomicBool = AtomicBool::new(false);
    let cpuid = unsafe { proc::cpuid() };
    if cpuid == 0 {
        println!();
        println!("osmium kernel is booting");
        println!();
        // kernel page table
        paging::kvminit();
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
        paging::kvminit();
    }
    println!("cpu#{} started", cpuid);
    loop {
        core::hint::spin_loop();
    }
}

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
        core::hint::spin_loop();
    }
}
