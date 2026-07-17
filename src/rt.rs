//! System runtime.

use core::arch::asm;

use crate::{
    driver::uart,
    mm::{self, PhysAddr},
    print, println,
};

#[unsafe(no_mangle)]
extern "C" fn kinit() -> usize {
    uart::initialize();
    mm::initialize();
    mm::id_map().expect("mapped memory");

    println!("{}", mm::framer());

    #[cfg(debug_assertions)]
    {
        let kernel_mem = mm::kmem();
        let alloc_list = kernel_mem.allocation_list();
        let kmem_start = alloc_list.head();
        let kmem_end = alloc_list.tail();
        unsafe {
            println!();
            println!("HEAP_START = 0x{:x}", mm::HEAP_START);
            println!("HEAP_SIZE = 0x{:x}", mm::HEAP_SIZE);
            println!("TEXT: 0x{:x} => 0x{:x}", mm::TEXT_START, mm::TEXT_END);
            println!("DATA: 0x{:x} => 0x{:x}", mm::DATA_START, mm::DATA_END);
            println!("RODATA: 0x{:x} => 0x{:x}", mm::RODATA_START, mm::RODATA_END);
            println!("BSS: 0x{:x} => 0x{:x}", mm::BSS_START, mm::BSS_END);
            println!(
                "KERNEL_STACK: 0x{:x} => 0x{:x}",
                mm::KERNEL_STACK_START,
                mm::KERNEL_STACK_END
            );
            println!("KERNEL_HEAP: 0x{:x} => 0x{:x}", kmem_start, kmem_end,);
            println!("MEMORY: 0x{:x} => 0x{:x}", mm::MEMORY_START, mm::MEMORY_END);
            println!();
        }
    }

    let p = 0x8005_7000_usize.into();
    let m = mm::kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {} = {}", p, m);

    let p = uart::BASE_ADDRESS.into();
    let m = mm::kmem().virt2phys(p).unwrap_or(PhysAddr::ZERO);
    println!("Walk {} = {}", p, m);

    let root_alloc_table_addr = {
        let kmem = mm::kmem();
        kmem.page_table_addr() as usize
    };
    (root_alloc_table_addr >> 12) | (8 << 60)
}

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    println!("hello, world!");
    println!("{}", mm::framer());

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
