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

use core::ptr::NonNull;

use osmium::{
    main,
    mem::{TRAMP_ADDR, TRAMPOLINE, vaddr::VirtAddr},
    paging, println,
};

main!(main);

/// The main Rust entry point of the kernel.
///
/// This function is called by the `boot` assembly code. It routes execution to the
/// test runner if tests are enabled, or to `kernel_main` for normal operation.
extern "C" fn main() {
    osmium::kinit();
    let kvm = paging::kvm();

    let vaddr1 = VirtAddr::new(TRAMPOLINE);
    let paddr1 = kvm
        .lock()
        .translate(vaddr1)
        .expect("address should be mapped");

    let vaddr2 = VirtAddr::new(unsafe { TRAMP_ADDR });
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
