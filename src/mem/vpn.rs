use core::{fmt, ops};

use crate::{addr::VirtAddr, mem::ppn::PhysPageNumber};

const VPN_BITS: usize = 27;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNumber(usize);

impl VirtPageNumber {
    pub fn identity_map(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0)
    }

    pub const fn indices(self) -> [usize; 3] {
        [self.0 & 0x1ff, self.0 >> 9 & 0x1ff, self.0 >> 18 & 0x1ff]
    }
}

impl ops::Add<usize> for VirtPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for VirtPageNumber {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl From<VirtPageNumber> for VirtAddr {
    fn from(ppn: VirtPageNumber) -> Self {
        Self::from(ppn.0 << 12)
    }
}

impl From<usize> for VirtPageNumber {
    fn from(bits: usize) -> Self {
        Self(bits & ((1 << VPN_BITS) - 1))
    }
}

impl fmt::Debug for VirtPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vpn@{:x}", self.0)
    }
}
