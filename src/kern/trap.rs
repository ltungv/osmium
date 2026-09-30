//! Kernel trap handlers.

use core::arch::naked_asm;

use crate::{
    println,
    rv::{
        InterruptCause, Privilege, TrapCause, scause, sepc,
        sstatus::{self, Sstatus},
        stvec,
    },
};

/// Install the supervisor trap vector which is located at [`kernelvec`].
pub fn inithart() {
    unsafe {
        stvec::write(kernelvec as *const () as usize);
    }
}

#[unsafe(link_section = ".tramp")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn trampoline() {
    naked_asm!("j trampoline")
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn kernelvec() {
    naked_asm!(
        // save stack space to hold the context
        "addi sp, sp, -128",
        // backup caller-saved registers.
        // return address
        "sd ra, 0(sp)",
        // global pointer
        "sd gp, 8(sp)",
        // temporaries
        "sd t0, 16(sp)",
        "sd t1, 24(sp)",
        "sd t2, 32(sp)",
        // arguments
        "sd a0, 40(sp)",
        "sd a1, 48(sp)",
        "sd a2, 56(sp)",
        "sd a3, 64(sp)",
        "sd a4, 72(sp)",
        "sd a5, 80(sp)",
        "sd a6, 88(sp)",
        "sd a7, 96(sp)",
        // temporaries
        "sd t3, 104(sp)",
        "sd t4, 112(sp)",
        "sd t5, 120(sp)",
        "sd t6, 128(sp)",
        // handle trap
        "call kerneltrap",
        // restore caller-saved registers
        // return address
        "ld ra, 0(sp)",
        // global pointer
        "ld gp, 8(sp)",
        // temporaries
        "ld t0, 16(sp)",
        "ld t1, 24(sp)",
        "ld t2, 32(sp)",
        // arguments
        "ld a0, 40(sp)",
        "ld a1, 48(sp)",
        "ld a2, 56(sp)",
        "ld a3, 64(sp)",
        "ld a4, 72(sp)",
        "ld a5, 80(sp)",
        "ld a6, 88(sp)",
        "ld a7, 96(sp)",
        // temporaries
        "ld t3, 104(sp)",
        "ld t4, 112(sp)",
        "ld t5, 120(sp)",
        "ld t6, 128(sp)",
        // return the used stack space
        "addi sp, sp, 128",
        "sret",
    )
}

#[unsafe(link_section = ".tramp")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn uservec() {
    naked_asm!("j uservec")
}

/// Traps from supervisor mode are handled here.
///
/// When a trap occurs in supervisor mode, the CPU jumps to [`kernelvec`] which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn kerneltrap() {
    let sstatus = unsafe { sstatus::read() };
    if sstatus.spp() != Privilege::Supervisor {
        panic!("kerneltrap - not from supervisor mode");
    }
    if sstatus.has(Sstatus::SIE) {
        panic!("kerneltrap - interrupts enabled");
    }
    let cause = unsafe { scause::read() };
    let TrapCause::Interrupt(intr) = cause else {
        println!("unexpected exception {cause:?}");
        panic!("kerneltrap");
    };
    let epc = unsafe { sepc::read() };
    match intr {
        InterruptCause::SupervisorExternal => {
            println!("e-intr");
        }
        InterruptCause::SupervisorTimer => {
            println!("t-intr");
        }
        _ => {
            println!("unexpected interrupt {intr:?}");
            panic!("kerneltrap");
        }
    }
    unsafe {
        sepc::write(epc);
        sstatus::write(sstatus);
    }
}

/// Traps from user mode are handled here.
///
/// When a trap occurs in user mode, the CPU jumps to [`uservec`] which will in turn call this
/// function to handle the trap.
#[unsafe(link_section = ".tramp")]
#[unsafe(no_mangle)]
extern "C" fn usertrap() {}
