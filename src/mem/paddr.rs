//! Physical memory address.

use core::fmt;

use crate::{
    PAGE_SIZE,
    mem::{align_up, ppn::PhysPageNumber, vaddr::VirtAddr},
};

/// A physical memory address.
///
/// On `riscv64`, only the 56 lower bits of a physical address are used. The top 8 bits
/// must be zero. This type guarantees that it always represents a valid physical address.
///
/// Using a distinct type for physical addresses prevents accidentally mixing them
/// with virtual addresses, enhancing type safety across the kernel memory subsystem.
///
/// # Examples
///
/// ```
/// let addr = PhysAddr::new(0x8000_0000);
/// let aligned = addr.align_up(4096);
/// assert_eq!(aligned, addr);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// The number of valid lower bits in a physical address.
    ///
    /// On `riscv64`, physical addresses are at most 56 bits wide. The upper 8 bits must be zero.
    const BITS: usize = 56;

    /// Assume this physical address is directly mapped to a virtual address of the same value.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the given physical address is directly mapped to a valid
    /// virtual address.
    pub const unsafe fn direct(self) -> VirtAddr {
        VirtAddr::new(self.0)
    }

    /// Creates a new physical address, asserting that the higher 8 bits are zero.
    pub const fn new(addr: usize) -> Self {
        Self::new_checked(addr).expect("invalid physical address")
    }

    /// Creates a new physical address, returning [`None`] if the higher 8 bits are non-zero.
    pub const fn new_checked(addr: usize) -> Option<Self> {
        let mask = (1 << Self::BITS) - 1;
        if addr == addr & mask {
            Some(Self(addr))
        } else {
            None
        }
    }

    /// Gets the page number of this physical address.
    pub const fn page_number(self) -> PhysPageNumber {
        PhysPageNumber::new(self.0 / PAGE_SIZE)
    }

    /// Calculates the offset between these two physical addresses.
    pub const fn offset_from(self, other: Self) -> Option<usize> {
        self.0.checked_sub(other.0)
    }

    /// Adds an offset to this physical address, wrapping around on overflow.
    pub const fn wrapping_add(self, len: usize) -> Self {
        Self::new(self.0.wrapping_add(len))
    }

    /// Align the address upwards to a multiple of `align`.
    pub const fn align_up(self, align: usize) -> Self {
        Self::new(align_up(self.0, align))
    }
}

impl fmt::Pointer for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phys@{:x}", self.0)
    }
}
