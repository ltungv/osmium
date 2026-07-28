//! System runtime.

use core::arch::asm;

use alloc::vec::Vec;

use crate::{
    HEAP_START,
    frame::{self, frame_allocator},
    kmem::{self, kmem},
    mem::PhysAddr,
    print, println, uart,
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
        let v = 0x0 as *mut u64;
        v.write_volatile(0);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
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

#[unsafe(no_mangle)]
extern "C" fn mtrap(
    epc: usize,
    tval: usize,
    cause: usize,
    hartid: usize,
    status: usize,
    frame: *mut TrapFrame,
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
                panic!("Unhandled async trap CPU#{} -> {}\n", hartid, cause_code);
            }
        }
    } else {
        // Synchronous trap
        match cause_code {
            2 => {
                // Illegal instruction
                panic!(
                    "Illegal instruction CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hartid, epc, tval
                );
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
                panic!(
                    "E-call from Machine mode! CPU#{} -> 0x{:08x}\n",
                    hartid, epc
                );
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
                panic!("Unhandled sync trap CPU#{} -> {}\n", hartid, cause_code);
            }
        }
    };
    // Finally, return the updated program counter
    program_counter
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
