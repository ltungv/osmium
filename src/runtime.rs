//! The kernel runtime

#[cfg(debug_assertions)]
use core::{arch::asm, panic::PanicInfo};

#[cfg(debug_assertions)]
use crate::mmu::{
    BSS_END, BSS_START, DATA_END, DATA_START, KERNEL_STACK_END, KERNEL_STACK_START, MEMORY_END,
    MEMORY_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
};
use crate::{
    mmu::{self, kmem, PageTableEntry, PageTableError, HEAP_SIZE, HEAP_START, PAGE_SIZE},
    uart::{self, UART_BASE_ADDRESS},
};
use crate::{print, println};

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
    uart::initialize();
    mmu::initialize();
    kmem::initialize(&mut mmu::page_allocator());
    map_memory().unwrap();

    #[cfg(debug_assertions)]
    {
        print_memory_layout();
        verify_identity_memory_layout();
    }

    let root_alloc_table_addr = {
        let mut kernel_memory = kmem::kmem();
        kernel_memory.page_table_addr() as usize
    };
    (root_alloc_table_addr >> 12) | (8 << 60)
}

#[no_mangle]
extern "C" fn kmain() -> ! {
    loop {
        let uart = uart::driver();
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

fn map_memory() -> Result<(), PageTableError> {
    let mut kernel_memory = kmem::kmem();
    let mut page_allocator = mmu::page_allocator();

    let (kmem_start, kmem_end) = {
        let alloc_list = kernel_memory.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    let root = unsafe { &mut *kernel_memory.page_table_addr() };

    root.id_map_range(
        &mut page_allocator,
        kmem_start,
        kmem_end,
        PageTableEntry::RW,
    )?;
    root.id_map_range(
        &mut page_allocator,
        UART_BASE_ADDRESS,
        UART_BASE_ADDRESS + 0x100,
        PageTableEntry::RW,
    )?;
    unsafe {
        root.id_map_range(
            &mut page_allocator,
            HEAP_START,
            HEAP_START + (HEAP_SIZE / PAGE_SIZE) * PAGE_SIZE,
            PageTableEntry::RW,
        )?;

        root.id_map_range(
            &mut page_allocator,
            TEXT_START,
            TEXT_END,
            PageTableEntry::RX,
        )?;

        root.id_map_range(
            &mut page_allocator,
            RODATA_START,
            RODATA_END,
            PageTableEntry::RX,
        )?;

        root.id_map_range(
            &mut page_allocator,
            DATA_START,
            DATA_END,
            PageTableEntry::RW,
        )?;

        root.id_map_range(&mut page_allocator, BSS_START, BSS_END, PageTableEntry::RW)?;

        root.id_map_range(
            &mut page_allocator,
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            PageTableEntry::RW,
        )?;
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn verify_identity_memory_layout_range(start: usize, end: usize) {
    let mut kernel_memory = kmem::kmem();
    let root_addr = kernel_memory.page_table_addr();
    let root = unsafe { &*root_addr };
    for vaddr in start..end {
        let paddr = root.v2p(vaddr).unwrap_or(0);
        assert_eq!(vaddr, paddr, "{vaddr:x} != {paddr:x}");
    }
}

#[cfg(debug_assertions)]
fn verify_identity_memory_layout() {
    let (kmem_start, kmem_end) = {
        let kernel_memory = kmem::kmem();
        let alloc_list = kernel_memory.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    unsafe {
        verify_identity_memory_layout_range(kmem_start, kmem_end);
        verify_identity_memory_layout_range(UART_BASE_ADDRESS, UART_BASE_ADDRESS + 0x100);
        verify_identity_memory_layout_range(
            HEAP_START,
            HEAP_START + (HEAP_SIZE / PAGE_SIZE) * PAGE_SIZE,
        );
        verify_identity_memory_layout_range(TEXT_START, TEXT_END);
        verify_identity_memory_layout_range(RODATA_START, RODATA_END);
        verify_identity_memory_layout_range(DATA_START, DATA_END);
        verify_identity_memory_layout_range(BSS_START, BSS_END);
        verify_identity_memory_layout_range(KERNEL_STACK_START, KERNEL_STACK_END);
    }
}

#[cfg(debug_assertions)]
fn print_memory_layout() {
    let kernel_mem = kmem::kmem();
    let alloc_list = kernel_mem.allocation_list();
    let kmem_start = alloc_list.head();
    let kmem_end = alloc_list.tail();
    unsafe {
        println!();
        println!("HEAP_START = 0x{:x}", HEAP_START);
        println!("HEAP_SIZE = 0x{:x}", HEAP_SIZE);
        println!("TEXT: 0x{:x} => 0x{:x}", TEXT_START, TEXT_END);
        println!("DATA: 0x{:x} => 0x{:x}", DATA_START, DATA_END);
        println!("RODATA: 0x{:x} => 0x{:x}", RODATA_START, RODATA_END);
        println!("BSS: 0x{:x} => 0x{:x}", BSS_START, BSS_END);
        println!(
            "KERNEL_STACK: 0x{:x} => 0x{:x}",
            KERNEL_STACK_START, KERNEL_STACK_END
        );
        println!("KERNEL_HEAP: 0x{:x} => 0x{:x}", kmem_start, kmem_end,);
        println!("MEMORY: 0x{:x} => 0x{:x}", MEMORY_START, MEMORY_END);
        println!();
    }
}
