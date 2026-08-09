use core::{fmt, ops};

use crate::addr::PhysAddr;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl PhysPageNumber {
    pub const BITS: usize = 44;

    pub const fn new_trunc(ppn: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(ppn & mask)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn addr(self) -> PhysAddr {
        PhysAddr::new_trunc(self.0 << 12)
    }
}

impl ops::Add<usize> for PhysPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for PhysPageNumber {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Pointer for PhysPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ppn@{:x}", self.0)
    }
}
