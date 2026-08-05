use core::{fmt, ops};

use crate::{PAGE_SIZE, mem::ppn::PhysPageNumber};

#[derive(Clone, Copy)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// Aligns this physical address to the next `exp`-byte boundary, and returns the aligned
    /// physical address.
    pub(crate) fn align(self, exp: usize) -> Self {
        assert!(exp.is_power_of_two());
        Self((self.0 + exp - 1) & !(exp - 1))
    }

    /// Returns the physical page number of the page after or at the current address.
    pub(crate) fn ceil(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0.div_ceil(PAGE_SIZE))
    }

    /// Returns the physical page number of the page containing the address.
    pub(crate) fn floor(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0 / PAGE_SIZE)
    }

    /// # SAFETY
    ///
    /// Casting a physical address into a raw pointer is generally unsafe because it's not guaranteed
    /// that the raw pointer points to the same physical memory. If the memory management unit is
    /// enabled, raw pointers is automatically translated into actual physical addresses. Additionally,
    /// the caller must make sure that the physical address is aligned to the alignment of the given
    /// type `T`.
    ///
    /// It's safe to cast a physical address into a raw pointer under two scenarios:
    /// * The memory management unit is disabled.
    /// * The memory management unit is enabled and the physical address has been identity mapped.
    pub(crate) const unsafe fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// # SAFETY
    ///
    /// Casting a physical address into a raw pointer is generally unsafe because it's not guaranteed
    /// that the raw pointer points to the same physical memory. If the memory management unit is
    /// enabled, raw pointers is automatically translated into actual physical addresses. Additionally,
    /// the caller must make sure that the physical address is aligned to the alignment of the given
    /// type `T`.
    ///
    /// It's safe to cast a physical address into a raw pointer under two scenarios:
    /// * The memory management unit is disabled.
    /// * The memory management unit is enabled and the physical address has been identity mapped.
    pub(crate) const unsafe fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

impl ops::Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<PhysAddr> for usize {
    fn from(addr: PhysAddr) -> Self {
        addr.0
    }
}

impl From<usize> for PhysAddr {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<PhysPageNumber> for PhysAddr {
    fn from(ppn: PhysPageNumber) -> Self {
        Self(usize::from(ppn) << 12)
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phys@{:x}", self.0)
    }
}

#[derive(Clone, Copy)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// Returns the offset of the virtual address, corresponding to the first `PAGE_SIZE_BITS` bits
    /// of the virtual address.
    pub(crate) const fn offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }
}

impl ops::Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<VirtAddr> for usize {
    fn from(addr: VirtAddr) -> Self {
        addr.0
    }
}

impl From<usize> for VirtAddr {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virt@{:x}", self.0)
    }
}
