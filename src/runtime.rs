//! The kernel runtime

#[cfg(debug_assertions)]
use core::{arch::asm, panic::PanicInfo};

#[cfg(debug_assertions)]
use crate::mmu::{
    BSS_END, BSS_START, DATA_END, DATA_START, KERNEL_STACK_END, KERNEL_STACK_START, MEMORY_END,
    MEMORY_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
};
use crate::{
    mmu::{self, kmem, PageTableEntry, HEAP_SIZE, HEAP_START, PAGE_SIZE},
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
    unsafe {
        uart::initialize();
        mmu::initialize();
        kmem::initialize(&mut mmu::page_allocator());

        #[cfg(debug_assertions)]
        {
            let (kmem_start, kmem_end) = {
                let m = kmem::kmem();
                let alloc_list = m.allocation_list();
                (alloc_list.head(), alloc_list.tail())
            };
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

        {
            let mut kernel_memory = kmem::kmem();
            let (kmem_start, kmem_end) = {
                let alloc_list = kernel_memory.allocation_list();
                (alloc_list.head(), alloc_list.tail())
            };
            let mut page_allocator = mmu::page_allocator();
            let root = &mut *kernel_memory.page_table_addr();

            root.id_map_range(
                &mut page_allocator,
                kmem_start,
                kmem_end,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                HEAP_START,
                HEAP_START + (HEAP_SIZE / PAGE_SIZE) * PAGE_SIZE,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                TEXT_START,
                TEXT_END,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                RODATA_START,
                RODATA_END,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                DATA_START,
                DATA_END,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                BSS_START,
                BSS_END,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                KERNEL_STACK_START,
                KERNEL_STACK_END,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);

            root.id_map_range(
                &mut page_allocator,
                UART_BASE_ADDRESS,
                UART_BASE_ADDRESS,
                PageTableEntry::READ | PageTableEntry::WRITE,
            )
            .unwrap();
            println!("{:?}", page_allocator);
        }

        let root_alloc_table_addr = {
            let mut kernel_memory = kmem::kmem();
            let (kmem_start, kmem_end) = {
                let alloc_list = kernel_memory.allocation_list();
                (alloc_list.head(), alloc_list.tail())
            };
            let root_addr = kernel_memory.page_table_addr();
            let root = &*root_addr;

            let p = 0x8005_7000;
            let m = root.v2p(p).unwrap_or(0);
            println!("Walk 0x{:x} = 0x{:x}", p, m);

            let p = kmem_start;
            let m = root.v2p(p).unwrap_or(0);
            println!("Walk 0x{:x} = 0x{:x}", p, m);

            let p = kmem_end;
            let m = root.v2p(p).unwrap_or(0);
            println!("Walk 0x{:x} = 0x{:x}", p, m);

            let p = UART_BASE_ADDRESS;
            let m = root.v2p(p).unwrap_or(0);
            println!("Walk 0x{:x} = 0x{:x}", p, m);

            root_addr as usize
        };

        // table / 4096    Sv39 mode
        (root_alloc_table_addr >> 12) | (8 << 60)
    }
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
