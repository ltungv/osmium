//! System runtime.

use core::arch::asm;

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START, frame, kmem, print,
    println,
    sv39::{self, EntryFlag, PhysAddr},
    uart,
};

#[unsafe(no_mangle)]
extern "C" fn kinit() -> usize {
    uart::initialize();
    frame::initialize();
    kmem::initialize();

    id_map().expect("mapped memory");
    println!("{}", frame::allocator());

    #[cfg(debug_assertions)]
    {
        let kmem = kmem::kmem();
        let alloc_list = kmem.allocation_list();
        let kmem_start = alloc_list.head();
        let kmem_end = alloc_list.tail();
        unsafe {
            use crate::{
                BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
                KERNEL_STACK_START, MEMORY_END, MEMORY_START, RODATA_END, RODATA_START, TEXT_END,
                TEXT_START,
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
    }

    let p = 0x8005_7000_usize.into();
    let m = kmem::kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {} = {}", p, m);

    let p = uart::BASE_ADDRESS.into();
    let m = kmem::kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {} = {}", p, m);

    let root_alloc_table_addr = kmem::kmem().page_table_addr() as usize;
    (root_alloc_table_addr >> 12) | (8 << 60)
}

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    println!("hello, world!");
    println!("{}", frame::allocator());

    // let p1 = mm::frame_allocator().zalloc(4).unwrap();
    // let p2 = mm::frame_allocator().zalloc(3).unwrap();
    // let p3 = mm::frame_allocator().zalloc(2).unwrap();
    // let p4 = mm::frame_allocator().zalloc(1).unwrap();

    // println!("{}", p1);
    // println!("{}", p2);
    // println!("{}", p3);
    // println!("{}", p4);
    // println!("{}", mm::frame_allocator());

    // unsafe { mm::frame_allocator().dealloc(p2) };
    // unsafe { mm::frame_allocator().dealloc(p3) };
    // println!("{}", mm::frame_allocator());

    // let p5 = mm::frame_allocator().zalloc(8).unwrap();
    // println!("{}", p5);
    // println!("{}", mm::frame_allocator());
}

#[unsafe(no_mangle)]
extern "C" fn eh_personality() {}

#[unsafe(no_mangle)]
extern "C" fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("aborting!");
    if let Some(p) = info.location() {
        println!("panic: {} ({}:{})", info.message(), p.file(), p.line());
    } else {
        println!("panic: no information available.");
    }
    abort();
}

/// Identity map all sections of the kernel's memory.
pub fn id_map() -> Result<(), sv39::TableError> {
    let (kmem_start, kmem_end) = {
        let kmem = kmem::kmem();
        let alloc_list = kmem.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    let root = unsafe { &mut *kmem::kmem().page_table_addr() };

    root.map(
        frame::allocator(),
        uart::BASE_ADDRESS.into(),
        uart::BASE_ADDRESS.into(),
        EntryFlag::default().set_readable(true).set_writeable(true),
        0,
    )?;

    root.id_map_range(
        frame::allocator(),
        kmem_start,
        kmem_end,
        EntryFlag::default().set_readable(true).set_writeable(true),
    )?;

    unsafe {
        root.id_map_range(
            frame::allocator(),
            HEAP_START,
            HEAP_START + HEAP_SIZE,
            EntryFlag::default().set_readable(true).set_writeable(true),
        )?;

        root.id_map_range(
            frame::allocator(),
            TEXT_START,
            TEXT_END,
            EntryFlag::default().set_readable(true).set_executable(true),
        )?;

        root.id_map_range(
            frame::allocator(),
            RODATA_START,
            RODATA_END,
            EntryFlag::default().set_readable(true).set_executable(true),
        )?;

        root.id_map_range(
            frame::allocator(),
            DATA_START,
            DATA_END,
            EntryFlag::default().set_readable(true).set_writeable(true),
        )?;

        root.id_map_range(
            frame::allocator(),
            BSS_START,
            BSS_END,
            EntryFlag::default().set_readable(true).set_writeable(true),
        )?;

        root.id_map_range(
            frame::allocator(),
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            EntryFlag::default().set_readable(true).set_writeable(true),
        )?;
    }
    Ok(())
}
