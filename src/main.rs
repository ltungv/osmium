//! A risc-v kernel.

#![feature(custom_test_frameworks)]
#![no_main]
#![no_std]
#![reexport_test_harness_main = "ktest"]
#![test_runner(kern::test::runner)]
#![warn(
    clippy::all,
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_core,
    missing_docs,
    rustdoc::all
)]

mod dev;
mod kern;
mod rv;
mod util;

start!(start);

#[cfg(test)]
extern "C" fn start() {
    kern::sinit();
    ktest();
}

#[cfg(not(test))]
extern "C" fn start() {
    kern::sinit();
    kern::sched();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    kern::test::panic(info);
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
    kern::abort()
}
