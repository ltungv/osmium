//! A RISCV kernel

#![no_std]
#![deny(missing_docs)]
#![warn(
    clippy::all,
    rustdoc::all,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![feature(panic_info_message)]

pub mod uart_ns16550;

use core::{arch::asm, panic::PanicInfo, str::from_utf8};

use uart_ns16550::{UartDriver, QEMU_VIRT_UART_MMIO_ADDRESS};

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    print!("Aborting: ");
    if let Some(p) = info.location() {
        println!(
            "line {}, file {}: {}",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    } else {
        println!("no information available.");
    }
    abort();
}

#[no_mangle]
extern "C" fn eh_personality() {}

#[no_mangle]
extern "C" fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

#[no_mangle]
extern "C" fn kmain() {
    let mut uart = unsafe {
        UartDriver::initialize_global(QEMU_VIRT_UART_MMIO_ADDRESS);
        UartDriver::new(QEMU_VIRT_UART_MMIO_ADDRESS)
    };

    println!("Hello world!");
    loop {
        let c = uart.get();
        println!("{}", c);
    }
}
