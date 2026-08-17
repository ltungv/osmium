use core::fmt;

use crate::{
    PAGE_SIZE,
    mem::{ppn::PhysPageNumber, vpn::VirtPageNumber},
};

pub const fn align_down(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    x & !(align - 1)
}

pub const fn align_up(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    (x + align - 1) & !(align - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    const BITS: usize = 56;

    pub const fn new_trunc(addr: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(addr & mask)
    }

    pub const fn page_number(self) -> PhysPageNumber {
        PhysPageNumber::new_trunc(self.0 / PAGE_SIZE)
    }

    pub const fn offset_from(self, other: Self) -> Option<usize> {
        self.0.checked_sub(other.0)
    }

    pub const fn wrapping_add(self, len: usize) -> Self {
        Self(self.0.wrapping_add(len))
    }

    pub const fn align_down(self, align: usize) -> Self {
        Self::new_trunc(align_down(self.0, align))
    }

    pub const fn align_up(self, align: usize) -> Self {
        Self::new_trunc(align_up(self.0, align))
    }
}

impl fmt::Pointer for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phys@{:x}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
    const BITS: usize = 39;

    pub const fn new_trunc(addr: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(addr & mask)
    }

    pub const fn page_number(self) -> VirtPageNumber {
        VirtPageNumber::new_trunc(self.0 / PAGE_SIZE)
    }

    pub const fn page_offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    pub const fn wrapping_add(self, len: usize) -> Self {
        Self(self.0.wrapping_add(len))
    }

    pub const fn align_down(self, align: usize) -> Self {
        Self::new_trunc(align_down(self.0, align))
    }

    pub const fn align_up(self, align: usize) -> Self {
        Self::new_trunc(align_up(self.0, align))
    }

    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    pub const fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

impl fmt::Pointer for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virt@{:x}", self.0)
    }
}

#[inline]
pub const unsafe fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    VirtAddr::new_trunc(paddr.0)
}
