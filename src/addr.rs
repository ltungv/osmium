use core::{fmt, ops};

use crate::{
    PAGE_SIZE,
    mem::{ppn::PhysPageNumber, vpn::VirtPageNumber},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    const BITS: usize = 56;

    pub const fn new_trunc(addr: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(addr & mask)
    }

    /// Aligns this physical address to the next `exp`-byte boundary, and returns the aligned
    /// physical address.
    pub const fn align(self, exp: usize) -> Option<Self> {
        if !exp.is_power_of_two() {
            return None;
        }
        Some(Self((self.0 + exp - 1) & !(exp - 1)))
    }

    /// Returns the physical page number of the page after or at the current address.
    pub const fn ceil(self) -> PhysPageNumber {
        PhysPageNumber::new_trunc(self.0.div_ceil(PAGE_SIZE))
    }

    /// Returns the physical page number of the page containing the address.
    pub const fn floor(self) -> PhysPageNumber {
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

    /// Returns the offset of the virtual address.
    pub const fn offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    /// Returns the physical page number of the page containing the address.
    pub const fn floor(self) -> VirtPageNumber {
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

/// Converts a physical address to a virtual address using the identity mapping scheme.
#[inline]
pub const fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    VirtAddr::new_trunc(paddr.0)
}
