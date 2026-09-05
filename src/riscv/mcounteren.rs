use core::arch::asm;

pub struct Mcounteren {
    bits: u32,
}

impl Mcounteren {
    pub const TM: Self = Self { bits: 1 << 1 };

    pub const fn with(mut self, counteren: Mcounteren) -> Self {
        self.bits |= counteren.bits;
        self
    }
}

pub unsafe fn read() -> Mcounteren {
    let bits: u32;
    unsafe {
        asm!("csrr {}, mcounteren", out(reg) bits);
    }
    Mcounteren { bits }
}

pub unsafe fn write(counteren: Mcounteren) {
    unsafe {
        asm!("csrw mcounteren, {}", in(reg) counteren.bits);
    }
}
