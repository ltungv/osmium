use core::arch::asm;

use crate::riscv::InterruptFlags;

pub unsafe fn write(flags: InterruptFlags) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) flags.bits());
    }
}
