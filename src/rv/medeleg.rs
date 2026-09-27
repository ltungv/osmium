use core::arch::asm;

use crate::rv::ExceptionFlags;

pub unsafe fn write(flags: ExceptionFlags) {
    unsafe {
        asm!("csrw medeleg, {}", in(reg) flags.bits());
    }
}
