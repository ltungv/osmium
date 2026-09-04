//! Kernel trap handlers.
use crate::{
    println,
    riscv::{
        InterruptCause, Privilege, Sstatus, TrapCause, r_scause, r_sepc, r_sstatus, w_sepc,
        w_sstatus, w_stvec,
    },
};

unsafe extern "C" {
    fn kernelvec();
}

/// Install the supervisor trap vector which is located at `kernelvec`.
pub fn inithart() {
    unsafe {
        w_stvec(kernelvec as *const () as usize);
    }
}

/// Traps from the supervisor are handled here.
///
/// When a trap occurs in supervisor mode, the CPU jumps to `kernelvec` which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn kerneltrap() {
    let sstatus = unsafe { r_sstatus() };
    if sstatus.get_spp() != Privilege::Supervisor {
        panic!("kerneltrap - not from supervisor mode");
    }
    if sstatus.has(Sstatus::SIE) {
        panic!("kerneltrap - interrupts enabled");
    }
    let scause = unsafe { r_scause() };
    let TrapCause::Interrupt(intr) = scause else {
        println!("unexpected exception {scause:?}");
        panic!("kerneltrap");
    };
    let sepc = unsafe { r_sepc() };
    match intr {
        InterruptCause::SupervisorExternal => {
            println!("t-intr");
        }
        InterruptCause::SupervisorTimer => {
            println!("e-intr");
        }
        _ => {
            println!("unexpected interrupt {intr:?}");
            panic!("kerneltrap");
        }
    }
    unsafe {
        w_sepc(sepc);
        w_sstatus(sstatus);
    }
}

/// Traps from the user are handled here.
///
/// When a trap occurs in user mode, the CPU jumps to `uservec` which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn usertrap() {}
