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
    /// First memory address in the .text section
    pub static TEXT_START: usize;
    /// Last memory address in the .text section
    pub static TEXT_END: usize;
    /// First memory address in the .rodata section
    pub static RODATA_START: usize;
    /// Last memory address in the .rodata section
    pub static RODATA_END: usize;
    /// First memory address in the .data section
    pub static DATA_START: usize;
    /// Last memory address in the .data section
    pub static DATA_END: usize;
    /// First memory address in the .bss section
    pub static BSS_START: usize;
    /// Last memory address in the .bss section
    pub static BSS_END: usize;
    /// First memory address in the .kernel_stack section
    pub static KERNEL_STACK_START: usize;
    /// Last memory address in the .kernel_stack section
    pub static KERNEL_STACK_END: usize;
    /// First memory address in the .heap section
    pub static HEAP_START: usize;
    /// Last memory address in the .heap section
    pub static HEAP_SIZE: usize;
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

        println!();
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

        let allocator = mmu::PageAllocator::new(HEAP_START, HEAP_SIZE, 12);
        allocator.initialize();

        let addr1 = allocator.alloc(2).unwrap();
        allocator.print_page_allocations();

        let addr2 = allocator.alloc(4).unwrap();
        allocator.print_page_allocations();

        let addr3 = allocator.alloc(8).unwrap();
        allocator.print_page_allocations();

        allocator.dealloc(addr2);
        allocator.print_page_allocations();

        let addr4 = allocator.alloc(2).unwrap();
        allocator.print_page_allocations();

        let addr5 = allocator.alloc(2).unwrap();
        allocator.print_page_allocations();

        allocator.dealloc(addr3);
        allocator.print_page_allocations();

        allocator.dealloc(addr1);
        allocator.print_page_allocations();

        allocator.dealloc(addr4);
        allocator.print_page_allocations();

        allocator.dealloc(addr5);
        allocator.print_page_allocations();

        let uart = UartDriver::new(QEMU_VIRT_UART_MMIO_ADDRESS);
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
}

#[no_mangle]
extern "C" fn kmain() -> ! {
    // TODO: separate initialization and long running tasks
    loop {}
}
