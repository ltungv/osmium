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
pub mod uart;

#[cfg(debug_assertions)]
use core::mem::size_of;
use core::{arch::asm, panic::PanicInfo};

#[cfg(debug_assertions)]
use mmu::{
    Page, BSS_END, BSS_START, DATA_END, DATA_START, KERNEL_STACK_END, KERNEL_STACK_START,
    MEMORY_END, MEMORY_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
};
use mmu::{HEAP_SIZE, HEAP_START};
use uart::{UartDriver, QEMU_VIRT_UART_MMIO_ADDRESS};

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
        let page_allocator = mmu::PageAllocator::new(HEAP_START, HEAP_SIZE, 12);
        page_allocator.initialize();

        #[cfg(debug_assertions)]
        {
            println!();
            println!("TEXT_START   = 0x{:x}", TEXT_START);
            println!("TEXT_END     = 0x{:x}", TEXT_END);
            println!("DATA_START   = 0x{:x}", DATA_START);
            println!("DATA_END     = 0x{:x}", DATA_END);
            println!("RODATA_START = 0x{:x}", RODATA_START);
            println!("RODATA_END   = 0x{:x}", RODATA_END);
            println!("BSS_START    = 0x{:x}", BSS_START);
            println!("BSS_END      = 0x{:x}", BSS_END);
            println!("KERNEL_STACK = 0x{:x}", KERNEL_STACK_START);
            println!("KERNEL_STACK = 0x{:x}", KERNEL_STACK_END);
            println!("HEAP_START   = 0x{:x}", HEAP_START);
            println!("HEAP_SIZE    = 0x{:x}", HEAP_SIZE);
            println!("MEMORY_START = 0x{:x}", MEMORY_START);
            println!("MEMORY_END   = 0x{:x}", MEMORY_END);

            let addr1 = page_allocator.alloc(2).unwrap();
            page_allocator.print_page_allocations();

            let addr2 = page_allocator.alloc(4).unwrap();
            page_allocator.print_page_allocations();

            let addr3 = page_allocator.alloc(8).unwrap();
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr2);
            page_allocator.print_page_allocations();

            let addr4 = page_allocator.alloc(2).unwrap();
            page_allocator.print_page_allocations();

            let addr5 = page_allocator.alloc(2).unwrap();
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr3);
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr1);
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr4);
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr5);
            page_allocator.print_page_allocations();

            let addr6 = page_allocator
                .zalloc(HEAP_SIZE / (size_of::<Page>() + (1usize << 12)))
                .unwrap();
            page_allocator.print_page_allocations();

            page_allocator.dealloc(addr6);
            page_allocator.print_page_allocations();
        }

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
