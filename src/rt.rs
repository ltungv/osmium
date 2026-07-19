//! System runtime.

use core::arch::asm;

use alloc::vec::Vec;

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
    frame::{self, frame_allocator},
    kmem::{self, kmem},
    print, println,
    sv39::{self, EntryFlags, PhysAddr},
    uart,
};

#[unsafe(no_mangle)]
extern "C" fn kinit() -> usize {
    frame::initialize();
    kmem::initialize();

    id_map().unwrap();

    #[cfg(debug_assertions)]
    {
        id_map_check();
        let (kmem_start, kmem_end) = {
            let m = kmem();
            let alloc_list = m.allocation_list();
            (alloc_list.head(), alloc_list.tail())
        };
        unsafe {
            use crate::{
                BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
                KERNEL_STACK_START, MEMORY_END, MEMORY_START, RODATA_END, RODATA_START, TEXT_END,
                TEXT_START,
            };

            println!("HEAP_START = 0x{:x}", HEAP_START);
            println!("HEAP_SIZE = {}", HEAP_SIZE);
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
        }
    }

    let p = unsafe { (HEAP_START).into() };
    let m = kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {:?} = {:?}", p, m);

    let p = uart::BASE_ADDRESS.into();
    let m = kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {:?} = {:?}", p, m);

    let root_alloc_table_addr = kmem().page_table_addr();
    (root_alloc_table_addr >> 12) | (8 << 60)
}

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    println!("hello, world!");
    println!("{:?}", frame_allocator());

    {
        let v: Vec<u8> = Vec::with_capacity(8);
        println!("{}", v.capacity());
    }
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
fn id_map() -> Result<(), sv39::Error> {
    let (kmem_start, kmem_end) = {
        let m = kmem();
        let alloc_list = m.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    let root = unsafe { &mut *kmem().page_table_ptr_mut() };

    root.map(
        frame::frame_allocator(),
        uart::BASE_ADDRESS.into(),
        uart::BASE_ADDRESS.into(),
        EntryFlags::READ | EntryFlags::WRITE,
        0,
    )?;

    root.id_map_range(
        frame::frame_allocator(),
        kmem_start,
        kmem_end,
        EntryFlags::READ | EntryFlags::WRITE,
    )?;

    unsafe {
        root.id_map_range(
            frame::frame_allocator(),
            HEAP_START,
            HEAP_START + HEAP_SIZE,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            TEXT_START,
            TEXT_END,
            EntryFlags::READ | EntryFlags::EXECUTE,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            RODATA_START,
            RODATA_END,
            EntryFlags::READ | EntryFlags::EXECUTE,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            DATA_START,
            DATA_END,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            BSS_START,
            BSS_END,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;
    }
    Ok(())
}

/// Identity map all sections of the kernel's memory.
fn id_map_check() {
    fn check(addr: usize) -> bool {
        let root = unsafe { &mut *kmem().page_table_ptr_mut() };
        root.virt2phys(addr.into())
            .is_some_and(|paddr| paddr == addr.into())
    }

    let (kmem_start, kmem_end) = {
        let m = kmem();
        let alloc_list = m.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    assert!(check(uart::BASE_ADDRESS,));
    assert!(check(kmem_start));
    assert!(check(kmem_end));
    unsafe {
        assert!(check(HEAP_START));
        assert!(check(TEXT_START));
        assert!(check(TEXT_END));
        assert!(check(RODATA_START));
        assert!(check(RODATA_END));
        assert!(check(DATA_START));
        assert!(check(DATA_END));
        assert!(check(BSS_START));
        assert!(check(BSS_END));
        assert!(check(KERNEL_STACK_START));
        assert!(check(KERNEL_STACK_END));
    }
}
