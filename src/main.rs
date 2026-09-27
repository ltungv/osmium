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

pub mod dev;
pub mod kern;
pub mod rv;
pub mod util;

start!(start);

extern "C" fn start() {
    kern::sinit();
    #[cfg(test)]
    {
        ktest();
    }
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
    kern::panic(info)
}
