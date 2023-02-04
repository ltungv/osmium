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

use core::{arch::asm, panic::PanicInfo};

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
        let c = match uart.get() {
            None => continue,
            Some(v) => v,
        };
        match c {
            // Backspace or delete
            0x08 | 0x7f => print!("{} {}", 8 as char, 8 as char),
            // Carriage-return or newline
            0x0A | 0x0D => println!(),
            // ANSI escape sequences
            0x1B => {
                match uart.get() {
                    Some(0x5b) => {}
                    _ => continue,
                };
                match uart.get() {
                    Some(b'A') => {
                        println!("That's the up arrow!");
                    }
                    Some(b'B') => {
                        println!("That's the down arrow!");
                    }
                    Some(b'C') => {
                        println!("That's the right arrow!");
                    }
                    Some(b'D') => {
                        println!("That's the left arrow!");
                    }
                    _ => {
                        println!("That's something else.....");
                    }
                };
            }
            // Everything else
            _ => print!("{}", c as char),
        }
    }
}
