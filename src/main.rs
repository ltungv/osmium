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

use crate::kern::{
    mm::{TRAMPOLINE, tramp_addr, vaddr::VirtAddr},
    vm,
};

start!(start);

extern "C" fn start() {
    kern::sinit();

    #[cfg(test)]
    {
        ktest();
    }

    let vaddr1 = VirtAddr::new(TRAMPOLINE);
    let vaddr2 = VirtAddr::new(tramp_addr());
    let kvm = vm::kvm();

    let paddr1 = kvm
        .lock()
        .translate(vaddr1)
        .expect("address should be mapped");

    let paddr2 = kvm
        .lock()
        .translate(vaddr1)
        .expect("address should be mapped");

    assert_eq!(paddr1, paddr2);
    println!("{vaddr1:p} --> {paddr1:p}");
    println!("{vaddr2:p} --> {paddr2:p}");

    loop {
        core::hint::spin_loop();
    }
}
