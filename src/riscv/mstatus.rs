use core::arch::asm;

use crate::riscv::Privilege;

pub struct Mstatus {
    bits: usize,
}

impl Mstatus {
    pub const fn with_mpp(mut self, privilege: Privilege) -> Self {
        self.bits &= !(0b11 << 11);
        self.bits |= (privilege as usize) << 11;
        self
    }
}

pub unsafe fn read() -> Mstatus {
    let bits: usize;
    unsafe {
        asm!("csrr {}, mstatus", out(reg) bits);
    }
    Mstatus { bits }
}

pub unsafe fn write(status: Mstatus) {
    unsafe {
        asm!("csrw mstatus, {}", in(reg) status.bits);
    }
}
