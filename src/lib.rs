//! A RISC-V kernel.

#![feature(alloc_error_handler)]
#![no_std]
#![warn(
    clippy::all,
    clippy::missing_safety_doc,
    clippy::nursery,
    clippy::pedantic,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    rust_2021_compatibility,
    rust_2024_compatibility,
    rustdoc::all
)]

extern crate alloc;

mod addr;
mod mem;
mod uart;

use core::{arch::asm, ptr::NonNull};

use alloc::vec::Vec;
use mem::buddy;

use crate::{
    addr::VirtAddr,
    mem::{kheap, ppn::PhysPageNumber},
};

/// The size of a page in bytes.
pub const PAGE_SIZE: usize = 1 << PAGE_ORDER;

/// The number of bits needed to represent the page size.
pub const PAGE_ORDER: usize = 12;

unsafe extern "C" {
    /// First memory address in the `.text` section.
    pub static TEXT_START: usize;

    /// Last memory address in the `.text` section.
    pub static TEXT_END: usize;

    /// First memory address in the `.rodata` section.
    pub static RODATA_START: usize;

    /// Last memory address in the `.rodata` section.
    pub static RODATA_END: usize;

    /// First memory address in the `.data` section.
    pub static DATA_START: usize;

    /// Last memory address in the `.data` section.
    pub static DATA_END: usize;

    /// First memory address in the `.bss` section.
    pub static BSS_START: usize;

    /// Last memory address in the `.bss` section.
    pub static BSS_END: usize;

    /// First memory address of the kernel's stack.
    pub static KERNEL_STACK_START: usize;

    /// Last memory address of the kernel's stack.
    pub static KERNEL_STACK_END: usize;

    /// First memory address of the heap.
    pub static HEAP_START: usize;

    /// Size of the heap in bytes.
    pub static HEAP_SIZE: usize;

    /// First memory address.
    pub static MEMORY_START: usize;

    /// Last memory address.
    pub static MEMORY_END: usize;
}

const NPAGES: usize = 16;

/// Kernel initialization routine. The bootloader (`boot.S`) jumps to this function after setting up
/// the device in machine mode in `_start`.
#[unsafe(no_mangle)]
pub extern "C" fn kinit() -> usize {
    uart::init();
    mem::initialize_frame_allocator();
    mem::initialize_kheap_allocator();
    println!("{:?}\n", buddy());

    println!("---------------------------------------------");
    println!("{:?}\n", buddy());
    println!("---------------------------------------------");

    let mut ppns: [Option<PhysPageNumber>; NPAGES] = [None; NPAGES];
    for (order, page) in ppns.iter_mut().take(12).enumerate().rev() {
        let ppn = buddy().alloc(order);
        *page = ppn;
        println!("alloc {ppn:?}");
    }

    println!("---------------------------------------------");
    println!("{:?}\n", buddy());
    println!("---------------------------------------------");

    for &ppn in ppns.iter().flatten() {
        buddy().dealloc(ppn);
        println!("dealloc {ppn:?}");
    }

    println!("---------------------------------------------");
    println!("{:?}\n", buddy());
    println!("---------------------------------------------");

    let p = unsafe { VirtAddr::from(HEAP_START) };
    let m = kheap().translate(p).unwrap_or_else(|| 0.into());
    println!("Walk {:?} = {:?}", p, m);

    let p = VirtAddr::from(uart::QEMU_ADDR);
    let m = kheap().translate(p).unwrap_or_else(|| 0.into());
    println!("Walk {:?} = {:?}", p, m);

    kheap().satp()
}

/// Kernel main runtime.
#[unsafe(no_mangle)]
pub extern "C" fn kmain() {
    println!("hello, world!");
    {
        let v1: Vec<u8> = Vec::with_capacity(8);
        let v2: Vec<u8> = Vec::with_capacity(8);
        let v3: Vec<u8> = Vec::with_capacity(8);
        println!("{:?}", kheap());

        drop(v2);
        println!("{:?}", kheap());

        let v4: Vec<u8> = Vec::with_capacity(64);
        println!("{:?}", kheap());

        drop(v1);
        drop(v3);
        drop(v4);
    }

    println!("triggering faults...");
    unsafe {
        // Set the next machine timer to fire.
        let mtimecmp = 0x0200_4000 as *mut u64;
        let mtime = 0x0200_bff8 as *const u64;
        // The frequency given by QEMU is 10_000_000 Hz, so this sets
        // the next interrupt to fire one second from now.
        mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);

        // Let's cause a page fault and see what happens. This should trap
        // to m_trap under trap.rs
        let v = NonNull::dangling();
        v.write_volatile(0);
    }
}

/// Context of the frame causing the trap.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TrapFrame {
    gp_regs: [usize; 32],
    fp_regs: [usize; 32],
    satp: usize,
    pc: usize,
    hartid: usize,
    qm: usize,
    pid: usize,
    mode: usize,
}

/// # Panics
///
/// This will panic when most traps occur because we haven't implemented a handler for them.
#[unsafe(no_mangle)]
pub extern "C" fn mtrap(
    epc: usize,
    tval: usize,
    cause: usize,
    hartid: usize,
    _status: usize,
    _frame: *mut TrapFrame,
) -> usize {
    let is_async = cause >> 63 & 1 == 1;
    // The cause contains the type of trap (sync, async) as well as the cause
    // number. So, here we narrow down just the cause number.
    let cause_code = cause & 0xfff;
    let mut program_counter = epc;
    if is_async {
        // Asynchronous trap
        match cause_code {
            3 => {
                // Machine software
                println!("Machine software interrupt CPU#{}", hartid);
            }
            7 => unsafe {
                // Machine timer
                let mtimecmp = 0x0200_4000 as *mut u64;
                let mtime = 0x0200_bff8 as *const u64;
                // The frequency given by QEMU is 10_000_000 Hz, so this sets
                // the next interrupt to fire one second from now.
                mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);
            },
            11 => {
                // Machine external (interrupt from Platform Interrupt Controller (PLIC))
                println!("Machine external interrupt CPU#{}", hartid);
            }
            _ => {
                panic!("Unhandled async trap CPU#{hartid} -> {cause_code}\n");
            }
        }
    } else {
        // Synchronous trap
        match cause_code {
            2 => {
                // Illegal instruction
                panic!("Illegal instruction CPU#{hartid} -> 0x{epc:08x}: 0x{tval:08x}\n");
            }
            8 => {
                // Environment (system) call from User mode
                println!("E-call from User mode! CPU#{} -> 0x{:08x}", hartid, epc);
                program_counter += 4;
            }
            9 => {
                // Environment (system) call from Supervisor mode
                println!(
                    "E-call from Supervisor mode! CPU#{} -> 0x{:08x}",
                    hartid, epc
                );
                program_counter += 4;
            }
            11 => {
                // Environment (system) call from Machine mode
                panic!("E-call from Machine mode! CPU#{hartid} -> 0x{epc:08x}\n");
            }
            // Page faults
            12 => {
                // Instruction page fault
                println!(
                    "Instruction page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hartid, epc, tval
                );
                program_counter += 4;
            }
            13 => {
                // Load page fault
                println!(
                    "Load page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hartid, epc, tval
                );
                program_counter += 4;
            }
            15 => {
                // Store page fault
                println!(
                    "Store page fault CPU#{} -> 0x{:08x}: 0x{:08x}",
                    hartid, epc, tval
                );
                program_counter += 4;
            }
            _ => {
                panic!("Unhandled sync trap CPU#{hartid} -> {cause_code}\n");
            }
        }
    }
    // Finally, return the updated program counter
    program_counter
}

/// The lang item `eh_personality` is a function used by the failure mechanisms of the compiler.
///
/// This is often mapped to GCC’s personality function (see the std implementation for more
/// information), but programs which don’t trigger a panic can be assured that this function is
/// never called. Additionally, a `eh_catch_typeinfo` static is needed for certain targets which
/// implement Rust panics on top of C++ exceptions.
#[unsafe(no_mangle)]
pub const extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("aborting!");
    if let Some(p) = info.location() {
        println!("panic: {} ({}:{})", info.message(), p.file(), p.line());
    } else {
        println!("panic: no information available.");
    }
    abort()
}

fn abort() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
