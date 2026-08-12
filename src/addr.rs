use core::{fmt, ops};

use crate::{
    PAGE_SIZE,
    mem::{ppn::PhysPageNumber, vpn::VirtPageNumber},
};

pub const fn align_down(addr: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    addr & !(align - 1)
}

pub const fn align_up(addr: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    (addr + align - 1) & !(align - 1)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    const BITS: usize = 56;

    pub const fn new_trunc(addr: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(addr & mask)
    }

    pub const fn align_up(self, align: usize) -> Self {
        Self::new_trunc(align_up(self.0, align))
    }

    pub const fn page_number(self) -> PhysPageNumber {
        PhysPageNumber::new_trunc(self.0 / PAGE_SIZE)
    }
}

impl ops::Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for PhysAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Pointer for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phys@{:x}", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
    const BITS: usize = 39;

    pub const fn new_trunc(addr: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(addr & mask)
    }

    pub const fn offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    pub const fn align_down(self, align: usize) -> Self {
        Self::new_trunc(align_down(self.0, align))
    }

    pub const fn align_up(self, align: usize) -> Self {
        Self::new_trunc(align_up(self.0, align))
    }

    pub const fn page_number(self) -> VirtPageNumber {
        VirtPageNumber::new_trunc(self.0 / PAGE_SIZE)
    }

    pub const unsafe fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    pub const unsafe fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

impl ops::Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for VirtAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Pointer for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virt@{:x}", self.0)
    }
}

#[inline]
pub const fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    VirtAddr::new_trunc(paddr.0)
}
