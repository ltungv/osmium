use core::{fmt, ops};

use crate::mem::{PAGE_SIZE, PAGE_SIZE_BITS, ppn::PhysPageNumber};

#[derive(Clone, Copy)]
pub struct PhysAddress(usize);

impl fmt::Debug for PhysAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P0x{:x}", self.0)
    }
}

impl ops::Add<usize> for PhysAddress {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<PhysAddress> for usize {
    fn from(addr: PhysAddress) -> Self {
        addr.0
    }
}

impl From<usize> for PhysAddress {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<PhysPageNumber> for PhysAddress {
    fn from(ppn: PhysPageNumber) -> Self {
        Self(usize::from(ppn) << PAGE_SIZE_BITS)
    }
}

impl PhysAddress {
    pub(crate) fn as_ptr(self) -> *const u8 {
        self.0 as *const u8
    }

    pub(crate) fn as_ptr_mut(self) -> *mut u8 {
        self.0 as *mut u8
    }

    pub(crate) fn ceil(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0.div_ceil(PAGE_SIZE))
    }

    pub(crate) fn floor(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0 / PAGE_SIZE)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VirtAddress(usize);

impl fmt::Debug for VirtAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V0x{:x}", self.0)
    }
}
impl From<VirtAddress> for usize {
    fn from(addr: VirtAddress) -> Self {
        addr.0
    }
}

impl From<usize> for VirtAddress {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl VirtAddress {
    pub(crate) fn offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }
}
