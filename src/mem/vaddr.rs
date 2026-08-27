//! Virtual memory address.

use core::fmt;

use crate::{
    PAGE_SIZE,
    mem::{align_up, vpn::VirtPageNumber},
};

/// A virtual memory address.
///
/// On `riscv64`, under sv39 paging scheme, only the 39 lower bits of a virtual address are used.
/// The address is sign-extended, i.e., all top bits must equal bit 38. This type guarantees that
/// it always represents a valid virtual address.
///
/// By encapsulating the address in a type, we ensure that address manipulations
/// (like alignment and offset calculations) are checked for validity, preventing bugs
/// caused by invalid address bit patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// The number of valid lower bits in a virtual address under the Sv39 paging scheme.
    ///
    /// Sv39 uses 39-bit virtual addresses, meaning the top 25 bits must be copies of bit 38
    /// (sign-extended) to form a valid 64-bit address.
    pub const BITS: usize = 39;

    /// Create a new virtual address, asserting that the address is sign-extended.
    pub const fn new(addr: usize) -> Self {
        Self::new_checked(addr).expect("virtual address should be sign-extended")
    }

    /// Create a new virtual address, returning [`None`] if the address is not sign-extended.
    pub const fn new_checked(addr: usize) -> Option<Self> {
        let shift = usize::BITS as usize - Self::BITS;
        let trunc = ((addr << shift).cast_signed() >> shift).cast_unsigned();
        if addr == trunc {
            Some(Self(addr))
        } else {
            None
        }
    }

    /// Gets the page number of this virtual address.
    pub const fn page_number(self) -> VirtPageNumber {
        VirtPageNumber::new((self.0 / PAGE_SIZE) & ((1 << 27) - 1))
    }

    /// Gets the offset, i.e., the lower 12 bits, of this virtual address.
    pub const fn page_offset(self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    /// Adds an offset to this virtual address, wrapping around on overflow.
    pub const fn wrapping_add(self, len: usize) -> Self {
        Self(self.0.wrapping_add(len))
    }

    /// Align the address upwards to a multiple of `align`.
    pub const fn align_up(self, align: usize) -> Self {
        Self::new(align_up(self.0, align))
    }

    /// Cast the address to a raw constant pointer of some type.
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Cast the address to a raw mutable pointer of some type.
    pub const fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

impl fmt::Pointer for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virt@{:x}", self.0)
    }
}
