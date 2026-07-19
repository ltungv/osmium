//! System runtime.

use core::arch::asm;

use alloc::vec::Vec;

use crate::{
    HEAP_START,
    frame::{self, frame_allocator},
    kmem::{self, kmem},
    print, println,
    sv39::PhysAddr,
    uart,
};

#[unsafe(no_mangle)]
extern "C" fn kinit() -> usize {
    uart::initialize();
    frame::initialize();
    kmem::initialize();

    #[cfg(debug_assertions)]
    {
        let (kmem_start, kmem_end) = kmem().mem_region();
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

    let root_frame_id = kmem().root_frame_id();
    (root_frame_id.addr() >> 12) | (8 << 60)
}

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    println!("hello, world!");
    println!("{:?}", frame_allocator());

    {
        let v1: Vec<u8> = Vec::with_capacity(8);
        let v2: Vec<u8> = Vec::with_capacity(8);
        let v3: Vec<u8> = Vec::with_capacity(8);
        println!("{:?}", kmem());

        drop(v2);
        println!("{:?}", kmem());

        let v4: Vec<u8> = Vec::with_capacity(64);
        println!("{:?}", kmem());

        drop(v1);
        drop(v3);
        drop(v4);
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
