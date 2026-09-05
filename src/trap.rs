//! Kernel trap handlers.
use crate::{
    println,
    riscv::{
        InterruptCause, Privilege, TrapCause, scause, sepc,
        sstatus::{self, Sstatus},
        stvec,
    },
};

unsafe extern "C" {
    fn kernelvec();
}

/// Install the supervisor trap vector which is located at `kernelvec`.
pub fn inithart() {
    unsafe {
        stvec::write(kernelvec as *const () as usize);
    }
}

/// Traps from the supervisor are handled here.
///
/// When a trap occurs in supervisor mode, the CPU jumps to `kernelvec` which will in turn call this
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
        sepc::write(epc);
        sstatus::write(sstatus);
    }
}

/// Traps from the user are handled here.
///
/// When a trap occurs in user mode, the CPU jumps to `uservec` which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn usertrap() {}
