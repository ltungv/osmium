//! A risc-v kernel.

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
mod proc;
mod riscv;
mod uart;

use core::arch::asm;

use alloc::slice;
use mem::frame_allocator;

use crate::{
    addr::{PhysAddr, phys_to_virt},
    mem::page_table,
    proc::cpuid,
    riscv::{
        r_menvcfg, r_mhartid, r_mstatus, r_sie, w_medeleg, w_menvcfg, w_mepc, w_mideleg, w_mstatus,
        w_pmpaddr0, w_pmpcfg0, w_satp, w_sie, w_tp,
    },
};

static DUMMY_TRAP_FRAME: TrapFrame = TrapFrame::new();

/// The size of a page in bytes.
const PAGE_SIZE: usize = 4096;

/// Distance between consecutive registers of the 16550 UART device.
const UART_STRIDE: u8 = 1;

/// First memory address of the 16550 UART device on the `virt` machine in `QEMU`
const UART_START: usize = 0x1000_0000;

/// Last memory address of the 16550 UART device on the `virt` machine in `QEMU`
const UART_END: usize = UART_START + 8 * UART_STRIDE as usize;

unsafe extern "C" {
    /// First memory address in the `.text` section.
    static TEXT_START: usize;

    /// Last memory address in the `.text` section.
    static TEXT_END: usize;

    /// First memory address in the `.rodata` section.
    static RODATA_START: usize;

    /// Last memory address in the `.rodata` section.
    static RODATA_END: usize;

    /// First memory address in the `.data` section.
    static DATA_START: usize;

    /// Last memory address in the `.data` section.
    static DATA_END: usize;

    /// First memory address in the `.bss` section.
    static BSS_START: usize;

    /// Last memory address in the `.bss` section.
    static BSS_END: usize;

    /// First memory address of the kernel's stack.
    static KERNEL_STACK_START: usize;

    /// Last memory address of the kernel's stack.
    static KERNEL_STACK_END: usize;

    /// First memory address of the heap.
    static HEAP_START: usize;

    /// Size of the heap in bytes.
    static HEAP_SIZE: usize;
}

// TODO: Initialize the timer.
#[unsafe(no_mangle)]
extern "C" fn boot() {
    unsafe {
        // Set `mstatus.MPP` to 1, so the CPU switch into supervisor mode after `mret` is called.
        w_mstatus({
            let mut mstatus = r_mstatus();
            mstatus &= !(0b11 << 11);
            mstatus |= 0b01 << 11;
            mstatus
        });

        // Set `mepc` to the address of `main`, so the CPU jumps to `main` after `mret` is called.
        w_mepc(main as *const () as usize);

        // Set `satp` to 0 to disable paging.
        w_satp(0);

        // Delegate all exceptions and interrupts to supervisor mode.
        w_medeleg(0xffff);
        w_mideleg(0xffff);

        // Set `sie` to enable specific interrupts:
        // 1 << 9: Supervisor external interrupt enable bit
        // 1 << 5: Supervisor timer interrupt enable bit
        w_sie(r_sie() | (1 << 9) | (1 << 5));

        // Give supervisor mode access to all physical memory.
        w_pmpaddr0(0x3f_ffff_ffff_ffff);
        w_pmpcfg0(0xf);

        // Enable hardware updates of page table entries' A and D bits.
        w_menvcfg(r_menvcfg() | (1 << 61));

        let hartid = r_mhartid();
        if hartid == 0 {
            // Initialize the bss memory section to 0. Only one CPU is responsible for writing, and
            // there always exists a CPU with the identifier 0.
            let bss = slice::from_raw_parts_mut(BSS_START as *mut u8, BSS_END - BSS_START);
            bss.fill(0);
        }

        // Set the thread pointer to the identifier of the current CPU.
        w_tp(hartid);

        // Switch to supervisor mode and jump to `main`.
        asm!("mret");
    }
}

extern "C" fn main() {
    if cpuid() == 0 {
        println!();
        println!("osmium kernel is booting");
        println!();

        mem::init_frame_allocator();
        mem::init_kheap();
        mem::init_page_table();
        frame_allocator().lock().debug_print();
        let satp = page_table().lock().satp();
        unsafe {
            asm!("csrw mscratch, {}", in(reg) (&raw const DUMMY_TRAP_FRAME as usize));
            asm!("csrw satp, {}", in(reg) satp);
        }
    }
    loop {
        core::hint::spin_loop();
    }
}

// /// Kernel main runtime.
// extern "C" fn kmain() {
//     println!("hello, world!");
//     {
//         let vaddr = VirtAddr::new_trunc(unsafe { HEAP_START });
//         let paddr = page_table()
//             .lock()
//             .translate(vaddr)
//             .unwrap_or(PhysAddr::new_trunc(0));
//
//         println!("{vaddr:p} --> {paddr:p}");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//     }
//     {
//         let v1: Vec<u8> = Vec::with_capacity(8);
//         let v2: Vec<u8> = Vec::with_capacity(8);
//         let v3: Vec<u8> = Vec::with_capacity(8);
//         println!("allocated v1 v2 v3");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//
//         drop(v2);
//         println!("dropped v2");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//
//         let v4: Vec<u8> = Vec::with_capacity(64);
//         println!("allocated v4");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//
//         drop(v1);
//         drop(v3);
//         drop(v4);
//         println!("dropped v1 v3 v4");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//
//         let v5: Vec<u8> = Vec::with_capacity(1);
//         println!("allocated v5");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//
//         drop(v5);
//         println!("dropped v5");
//         kheap().lock().debug_print();
//         println!("-------------------------------");
//     }
//     println!("triggering faults...");
//     unsafe {
//         // Set the next machine timer to fire.
//         let mtimecmp = phys_to_virt(PhysAddr::new_trunc(0x0200_4000)).as_ptr_mut::<u64>();
//         let mtime = phys_to_virt(PhysAddr::new_trunc(0x0200_bff8)).as_ptr::<u64>();
//         println!("mtimecmp={mtimecmp:p} mtime={mtime:p}");
//         // The frequency given by QEMU is 10_000_000 Hz, so this sets
//         // the next interrupt to fire one second from now.
//         mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);
//         println!("mtimecmp={mtimecmp:p} mtime={mtime:p}");
//         // Let's cause a page fault and see what happens. This should trap
//         // to m_trap under trap.rs
//         // let v = NonNull::dangling();
//         // v.write_volatile(0);
//     }
//
//     loop {
//         spin_loop();
//     }
// }

/// Context of the frame causing the trap.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct TrapFrame {
    gp_regs: [usize; 32],
    fp_regs: [usize; 32],
    satp: usize,
    pc: usize,
    hartid: usize,
    qm: usize,
    pid: usize,
    mode: usize,
}

impl TrapFrame {
    const fn new() -> Self {
        Self {
            gp_regs: [0; 32],
            fp_regs: [0; 32],
            satp: 0,
            pc: 0,
            hartid: 0,
            qm: 0,
            pid: 0,
            mode: 0,
        }
    }
}

/// # Panics
///
/// This will panic when most traps occur because we haven't implemented a handler for them.
#[unsafe(no_mangle)]
extern "C" fn mtrap(
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
                let mtimecmp = phys_to_virt(PhysAddr::new_trunc(0x0200_4000)).as_ptr_mut::<u64>();
                let mtime = phys_to_virt(PhysAddr::new_trunc(0x0200_bff8)).as_ptr::<u64>();
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
const extern "C" fn eh_personality() {}

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

/// Errors that occur when working with the page table.
#[derive(Debug)]
enum Error {
    /// The kernel and/or its subsystems are in an invalid state.
    InvalidState,

    /// There's no memory left on the device for the kernel.
    OutOfMemory,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidState => write!(f, "invalid state"),
            Self::OutOfMemory => write!(f, "out of memory"),
        }
    }
}
