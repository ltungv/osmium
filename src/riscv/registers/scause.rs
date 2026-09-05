use core::arch::asm;

use crate::riscv::TrapCause;

pub unsafe fn read() -> TrapCause {
    let cause: usize;
    unsafe {
        asm!("csrr {}, scause", out(reg) cause);
    }
    TrapCause::from_code(cause).expect("scause should contain only known bits")
}
