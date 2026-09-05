use core::arch::asm;

use crate::mem::ppn::PhysPageNumber;

pub struct Satp {
    bits: usize,
}

impl Satp {
    pub const fn bare() -> Self {
        Self { bits: 0 }
    }

    pub const fn sv39(ppn: PhysPageNumber) -> Self {
        Self {
            bits: 0b0100 << 60 | ppn.get(),
        }
    }
}

pub unsafe fn write(atp: Satp) {
    unsafe {
        asm!("csrw satp, {}", in(reg) atp.bits);
    }
}
