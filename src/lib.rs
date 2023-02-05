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

pub mod mmu;
pub mod uart_ns16550;

use core::{arch::asm, panic::PanicInfo};

use uart_ns16550::{UartDriver, QEMU_VIRT_UART_MMIO_ADDRESS};

extern "C" {
    static TEXT_START: usize;
    static TEXT_END: usize;
    static RODATA_START: usize;
    static RODATA_END: usize;
    static DATA_START: usize;
    static DATA_END: usize;
    static BSS_START: usize;
    static BSS_END: usize;
    static KERNEL_STACK_START: usize;
    static KERNEL_STACK_END: usize;
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
    static mut KERNEL_TABLE: usize;
}

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
extern "C" fn kinit() -> usize {
    unsafe {
        UartDriver::initialize_global(QEMU_VIRT_UART_MMIO_ADDRESS);
        println!("TEXT_START   = {}", TEXT_START);
        println!("TEXT_END     = {}", TEXT_END);
        println!("DATA_START   = {}", DATA_START);
        println!("DATA_END     = {}", DATA_END);
        println!("RODATA_START = {}", RODATA_START);
        println!("RODATA_END   = {}", RODATA_END);
        println!("BSS_START    = {}", BSS_START);
        println!("BSS_END      = {}", BSS_END);
        println!("KERNEL_STACK = {}", KERNEL_STACK_START);
        println!("KERNEL_STACK = {}", KERNEL_STACK_END);
        println!("HEAP_START   = {}", HEAP_START);
        println!("HEAP_SIZE    = {}", HEAP_SIZE);
        println!("KERNEL_TABLE = {}", KERNEL_TABLE);
    }
    let mut uart = unsafe { UartDriver::new(QEMU_VIRT_UART_MMIO_ADDRESS) };
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

#[no_mangle]
extern "C" fn kmain() -> ! {
    // TODO: separate initialization and long running tasks
    loop {}
}
