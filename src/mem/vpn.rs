use core::{fmt, ops};

use crate::mem::{PAGE_SIZE_BITS, addr::VirtAddress};

const VPN_BITS: usize = 27;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct VirtPageNumber(usize);

impl fmt::Debug for VirtPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VPN(0x{:x})", self.0)
    }
}

impl ops::Add<usize> for VirtPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<VirtPageNumber> for usize {
    fn from(vpn: VirtPageNumber) -> Self {
        vpn.0
    }
}

impl From<usize> for VirtPageNumber {
    fn from(bits: usize) -> Self {
        Self(bits & ((1 << VPN_BITS) - 1))
    }
}

impl From<VirtAddress> for VirtPageNumber {
    fn from(addr: VirtAddress) -> Self {
        Self(usize::from(addr) >> PAGE_SIZE_BITS)
    }
}

impl VirtPageNumber {
    pub(crate) fn indices(self) -> [usize; 3] {
        [self.0 & 0x1ff, self.0 >> 9 & 0x1ff, self.0 >> 18 & 0x1ff]
    }
}
