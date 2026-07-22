//! Memory addresses

use core::ops::Add;

/// A physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub usize);

impl<T> From<*const T> for PhysAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for PhysAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl From<PhysAddr> for usize {
    fn from(addr: PhysAddr) -> Self {
        addr.0
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl PhysAddr {
    /// The zero physical address.
    pub const ZERO: Self = Self(0);

    /// Converts the physical address to a raw pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the physical address points to a valid object.
    pub unsafe fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    /// Converts the physical address to a raw const pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the physical address points to a valid object.
    pub unsafe fn as_const_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Decomposes the physical address into physical page numbers (PPNs).
    pub fn ppns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x3ff_ffff,
        ]
    }
}

/// A virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl<T> From<*const T> for VirtAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl From<VirtAddr> for usize {
    fn from(addr: VirtAddr) -> Self {
        addr.0
    }
}

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl VirtAddr {
    /// The zero virtual address.
    pub const ZERO: Self = Self(0);

    /// Decompose the virtual address into virtual page numbers (VPNs).
    pub fn vpns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x1ff,
        ]
    }
}
