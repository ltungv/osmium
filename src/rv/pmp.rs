use core::arch::asm;

pub struct PmpCfg {
    bits: u8,
}

impl PmpCfg {
    pub const R: Self = Self { bits: 1 << 0 };
    pub const W: Self = Self { bits: 1 << 1 };
    pub const X: Self = Self { bits: 1 << 2 };

    pub const fn napot() -> Self {
        Self { bits: 0b11 << 3 }
    }

    pub const fn with(mut self, cfg: Self) -> Self {
        self.bits |= cfg.bits;
        self
    }
}

pub unsafe fn write0(addr: usize, cfg: PmpCfg) {
    unsafe {
        asm!("csrw pmpaddr0, {}", in(reg) addr);
        asm!("csrw pmpcfg0, {}", in(reg) cfg.bits);
    }
}
