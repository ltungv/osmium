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

#[cfg(not(test))]
extern "C" fn start() {
    kern::sinit();
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
extern "C" fn start() {
    kern::sinit();
    ktest();
}
