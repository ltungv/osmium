use core::arch::asm;

pub struct Menvcfg {
    bits: usize,
}

impl Menvcfg {
    pub const ADUE: Self = Self { bits: 1 << 61 };
    pub const STCE: Self = Self { bits: 1 << 63 };

    pub const fn with(mut self, cfg: Self) -> Self {
        self.bits |= cfg.bits;
        self
    }
}

pub unsafe fn read() -> Menvcfg {
    let bits: usize;
    unsafe {
        asm!("csrr {}, menvcfg", out(reg) bits);
    }
    Menvcfg { bits }
}

pub unsafe fn write(cfg: Menvcfg) {
    unsafe {
        asm!("csrw menvcfg, {}", in(reg) cfg.bits);
    }
}
