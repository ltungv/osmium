use core::arch::asm;

use crate::rv::InterruptFlags;

pub unsafe fn read() -> InterruptFlags {
    let bits: usize;
    unsafe {
        asm!("csrr {}, sie", out(reg) bits);
    }
    InterruptFlags::from_bits_retain(bits)
}

pub unsafe fn write(flags: InterruptFlags) {
    unsafe {
        asm!("csrw sie, {}", in(reg) flags.bits());
    }
}
