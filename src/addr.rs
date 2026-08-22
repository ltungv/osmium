use core::{fmt, ops};

use crate::PAGE_SIZE;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl PhysPageNumber {
    pub const BITS: usize = 44;

    pub const fn new_trunc(ppn: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(ppn & mask)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn addr(self) -> PhysAddr {
        PhysAddr::new_trunc(self.0 << 12)
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNumber(usize);

impl VirtPageNumber {
    pub const BITS: usize = 27;

    pub const fn new_trunc(vpn: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(vpn & mask)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn addr(self) -> VirtAddr {
        VirtAddr::new_trunc(self.0 << 12)
    }

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

    pub const unsafe fn direct(addr: PhysAddr) -> Self {
        Self::new_trunc(addr.0)
    }

    pub const fn new_trunc(addr: usize) -> Self {
        let shift = usize::BITS as usize - Self::BITS;
        Self(((addr << shift).cast_signed() >> shift).cast_unsigned())
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

pub const fn align_down(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    x & !(align - 1)
}

pub const fn align_up(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    (x + align - 1) & !(align - 1)
}
