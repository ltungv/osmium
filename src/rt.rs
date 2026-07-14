//! System runtime.

use core::arch::asm;

use crate::{driver::uart, mm, print, println};

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    uart::initialize();

    println!("hello, world!");
    println!("{}", mm::frame_allocator());

    let p1 = mm::frame_allocator().zalloc(4).unwrap();
    let p2 = mm::frame_allocator().zalloc(3).unwrap();
    let p3 = mm::frame_allocator().zalloc(2).unwrap();
    let p4 = mm::frame_allocator().zalloc(1).unwrap();

    println!("{}", p1);
    println!("{}", p2);
    println!("{}", p3);
    println!("{}", p4);
    println!("{}", mm::frame_allocator());

    unsafe { mm::frame_allocator().dealloc(p2) };
    unsafe { mm::frame_allocator().dealloc(p3) };
    println!("{}", mm::frame_allocator());

    let p5 = mm::frame_allocator().zalloc(8).unwrap();
    println!("{}", p5);
    println!("{}", mm::frame_allocator());
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
