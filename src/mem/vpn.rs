//! Virtual page number.

use core::{fmt, ops};

use crate::mem::vaddr::VirtAddr;

/// The virtual page number.
///
/// On `riscv64`, only the 27 lower bits of a virtual page number are used since a virtual address
/// only uses its 39 lower bits. This type guarantees that it always represents a valid virtual
/// page number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNumber(usize);

impl VirtPageNumber {
    /// The number of valid lower bits in a virtual page number under the Sv39 paging scheme.
    ///
    /// Since Sv39 uses 39-bit virtual addresses and 12-bit page offsets, the virtual page
    /// number consists of the remaining 27 bits.
    const BITS: usize = 27;

    /// Create a new virtual page number, asserting that the higher 37 bits are zero.
    pub const fn new(vpn: usize) -> Self {
        Self::new_checked(vpn).expect("virtual page number should be truncated")
    }

    /// Create a new virtual page number, returning [`None`] if the higher 37 bits are not zero.
    pub const fn new_checked(vpn: usize) -> Option<Self> {
        let mask = (1 << Self::BITS) - 1;
        if vpn == vpn & mask {
            Some(Self(vpn))
        } else {
            None
        }
    }

    /// Get the virtual page number as a [`usize`] value.
    pub const fn get(self) -> usize {
        self.0
    }

    /// Get the virtual address of this page.
    pub const fn addr(self) -> VirtAddr {
        let addr = self.0 << 12;
        let shift = usize::BITS as usize - VirtAddr::BITS;
        VirtAddr::new(((addr << shift).cast_signed() >> shift).cast_unsigned())
    }

    /// Return the indices into the page tables in sv39 paging scheme.
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

impl fmt::Pointer for VirtPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vpn@{:x}", self.0)
    }
}
