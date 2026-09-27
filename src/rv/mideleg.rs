use core::arch::asm;

use crate::rv::InterruptFlags;

pub unsafe fn write(flags: InterruptFlags) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) flags.bits());
    }
}
