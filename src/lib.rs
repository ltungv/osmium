//! a risc-v kernel.

#![no_std]
#![warn(
    clippy::all,
    rustdoc::all,
    missing_docs,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2021_compatibility,
    rust_2024_compatibility
)]

mod driver;

use core::arch::asm;

use crate::driver::uart;

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    uart::initialize();
    println!("hello, world!");
}

#[unsafe(no_mangle)]
extern "C" fn eh_personality() {}

#[unsafe(no_mangle)]
extern "C" fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    print!("aborting: ");
    if let Some(p) = info.location() {
        println!("line {}, file {}: {}", p.line(), p.file(), info.message());
    } else {
        println!("no information available.");
    }
    abort();
}
