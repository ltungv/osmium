//! Types for addressing virtual and physical memory.

use core::{fmt, ops};

use crate::PAGE_SIZE;

/// Align the value `x` downwards to a multiple of `align`.
pub const fn align_down(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    x & !(align - 1)
}

/// Align the value `x` upwards to a multiple of `align`.
pub const fn align_up(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    (x + align - 1) & !(align - 1)
}

/// A physical memory address.
///
/// On `riscv64`, only the 56 lower bits of a physical address are used. The top 8 bits
/// must be zero. This type guarantees that it always represents a valid physical address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    const BITS: usize = 56;

    /// Creates a new physical address, asserting that the higher 8 bits are zero.
    pub const fn new(addr: usize) -> Self {
        Self::new_checked(addr).expect("invalid physical address")
    }

    /// Creates a new physical address, returning `None` if the higher 8 bits are non-zero.
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

/// A virtual memory address.
///
/// On `riscv64`, under sv39 paging scheme, only the 39 lower bits of a virtual address are used.
/// The address is sign-extended, i.e., all top bits must equal bit 38. This type guarantees that
/// it always represents a valid virtual address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
    const BITS: usize = 39;

    /// Create a new virtual address assuming it maps to a physical address of the same value.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the given physical address is directly mapped to a valid
    /// virtual address.
    pub const unsafe fn direct(addr: PhysAddr) -> Self {
        Self::new(addr.0)
    }

    /// Create a new virtual address, asserting that the address is sign-extended.
    pub const fn new(addr: usize) -> Self {
        Self::new_checked(addr).expect("invalid virtual address")
    }

    /// Create a new virtual address, returning `None` if the address is not sign-extended.
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
        VirtPageNumber::new((self.0 / PAGE_SIZE) & ((1 << VirtPageNumber::BITS) - 1))
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

/// A physical page number.
///
/// A page consists of a contiguous region of 4096 bytes and always starts at a 4096-byte aligned
/// physical address. As a result, the physical page number of a page containing some physical
/// address can be derived by dividing the address by 4096.
///
/// On `riscv64`, only the 44 lower bits of a physical page number are used since a physical address
/// only uses its 56 lower bits. This type guarantees that it always represents a valid physical
/// page number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl PhysPageNumber {
    /// The number of valid lower bits in a physical page number.
    pub const BITS: usize = 44;

    /// Create a new physical page number, asserting that the higher 20 bits are zero.
    pub const fn new(ppn: usize) -> Self {
        Self::new_checked(ppn).expect("invalid physical page number")
    }

    /// Create a new physical page number, returning `None` if the higher 20 bits are not zero.
    pub const fn new_checked(ppn: usize) -> Option<Self> {
        let mask = (1 << Self::BITS) - 1;
        if ppn == ppn & mask {
            Some(Self(ppn))
        } else {
            None
        }
    }

    /// Get the physical page number as a `usize` value.
    pub const fn get(self) -> usize {
        self.0
    }

    /// Get the physical address of this page.
    pub const fn addr(self) -> PhysAddr {
        PhysAddr::new(self.0 << 12)
    }
}

impl ops::Add<usize> for PhysPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for PhysPageNumber {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Pointer for PhysPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ppn@{:x}", self.0)
    }
}

/// The virtual page number.
///
/// On `riscv64`, only the 27 lower bits of a virtual page number are used since a virtual address
/// only uses its 39 lower bits. This type guarantees that it always represents a valid virtual
/// page number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNumber(usize);

impl VirtPageNumber {
    const BITS: usize = 27;

    /// Create a new virtual page number, asserting that the higher 37 bits are zero.
    pub const fn new(vpn: usize) -> Self {
        Self::new_checked(vpn).expect("invalid virtual page number")
    }

    /// Create a new virtual page number, returning `None` if the higher 37 bits are not zero.
    pub const fn new_checked(vpn: usize) -> Option<Self> {
        let mask = (1 << Self::BITS) - 1;
        if vpn == vpn & mask {
            Some(Self(vpn))
        } else {
            None
        }
    }

    /// Get the virtual page number as a `usize` value.
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
