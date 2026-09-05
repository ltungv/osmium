use core::arch::asm;

use crate::riscv::Privilege;

pub struct Sstatus {
    bits: usize,
}

impl Sstatus {
    pub const SIE: Self = Self { bits: 1 << 1 };

    pub const fn has(&self, status: Sstatus) -> bool {
        self.bits & status.bits == status.bits
    }

    pub const fn get_spp(&self) -> Privilege {
        let mask = 1 << 8;
        if self.bits & mask == mask {
            Privilege::Supervisor
        } else {
            Privilege::User
        }
    }
}

pub unsafe fn read() -> Sstatus {
    let bits: usize;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) bits);
    }
    Sstatus { bits }
}

pub unsafe fn read_clear(mut status: Sstatus) -> Sstatus {
    unsafe {
        asm!("csrrc {}, sstatus, {}", out(reg) status.bits, in(reg) status.bits);
    }
    status
}

pub unsafe fn set(status: Sstatus) {
    unsafe {
        asm!("csrs sstatus, {}", in(reg) status.bits);
    }
}

pub unsafe fn write(status: Sstatus) {
    unsafe {
        asm!("csrw sstatus, {}", in(reg) status.bits);
    }
}
