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

use core::sync::atomic::{self, AtomicBool};

use osmium::{boot, kheap, paging, println, proc};

boot!(main);

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
