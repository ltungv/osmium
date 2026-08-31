//! Physical page number.

use core::{fmt, ops};

use crate::{mem::paddr::PhysAddr, paging::page_table::PageTable};

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
    ///
    /// Since physical addresses are 56 bits and page offsets are 12 bits, the physical
    /// page number occupies the remaining 44 bits.
    pub const BITS: usize = 44;

    /// Create a new physical page number, asserting that the higher 20 bits are zero.
    pub const fn new(ppn: usize) -> Self {
        Self::new_checked(ppn).expect("physical page number should be truncated")
    }

    /// Create a new physical page number, returning [`None`] if the higher 20 bits are not zero.
    pub const fn new_checked(ppn: usize) -> Option<Self> {
        let mask = (1 << Self::BITS) - 1;
        if ppn == ppn & mask {
            Some(Self(ppn))
        } else {
            None
        }
    }

    /// Get the physical page number as a [`usize`] value.
    pub const fn get(self) -> usize {
        self.0
    }

    /// Get the physical address of this page.
    pub const fn addr(self) -> PhysAddr {
        PhysAddr::new(self.0 << 12)
    }

    /// Get a constant reference to the [`PageTable`] stored at this physical page number
    ///
    /// # Safety
    ///
    /// * It must hold that the system's physical memory is available in the kernel's virtual
    ///   address space through a direct-map.
    /// * It must hold that the system's physical memory at the address given by this page number
    ///   holds data of a valid and initialized PageTable.
    pub unsafe fn page_table(self) -> &'static PageTable {
        let paddr = self.addr();
        let vaddr = unsafe { paddr.direct() };
        let ptr = vaddr.as_ptr();
        unsafe { &*ptr }
    }

    /// Get a mutable reference to the [`PageTable`] stored at this physical page number
    ///
    /// # Safety
    ///
    /// * It must hold that the system's physical memory is available in the kernel's virtual
    ///   address space through a direct-map.
    /// * It must hold that the system's physical memory at the address given by this page number
    ///   holds data of a valid and initialized PageTable.
    pub unsafe fn page_table_mut(self) -> &'static mut PageTable {
        let paddr = self.addr();
        let vaddr = unsafe { paddr.direct() };
        let ptr = vaddr.as_ptr_mut();
        unsafe { &mut *ptr }
    }
}

impl ops::Add<usize> for PhysPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.0 + rhs)
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
