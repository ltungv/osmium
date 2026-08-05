use core::{fmt, ops, slice};

use crate::{
    PAGE_SIZE,
    addr::PhysAddr,
    mem::{
        page::{PageTable, PageTableEntry, PteFlags},
        vpn::VirtPageNumber,
    },
};

const PPN_BITS: usize = 44;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl PhysPageNumber {
    pub const fn satp(self) -> usize {
        8 << 60 | self.0
    }

    pub fn blend(self, lower: Self, mask: usize) -> Self {
        let lower = lower.0 & mask;
        let upper = self.0 & !mask;
        Self::from(upper | lower)
    }

    pub fn identity_map(self) -> VirtPageNumber {
        VirtPageNumber::from(self.0)
    }

    pub fn as_pte(self, flags: PteFlags) -> PageTableEntry {
        PageTableEntry::from(self.0 << 10 | flags.bits())
    }

    pub fn as_slice<T>(self) -> &'static [T] {
        unsafe {
            slice::from_raw_parts(
                PhysAddr::from(self).as_ptr::<T>(),
                PAGE_SIZE / size_of::<T>(),
            )
        }
    }

    pub fn as_slice_mut<T>(self) -> &'static mut [T] {
        unsafe {
            slice::from_raw_parts_mut(
                PhysAddr::from(self).as_ptr_mut::<T>(),
                PAGE_SIZE / size_of::<T>(),
            )
        }
    }

    pub fn as_page_table(self) -> &'static PageTable {
        unsafe { &*PhysAddr::from(self).as_ptr::<PageTable>() }
    }

    pub fn as_page_table_mut(self) -> &'static mut PageTable {
        unsafe { &mut *PhysAddr::from(self).as_ptr_mut::<PageTable>() }
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

impl From<PhysPageNumber> for PhysAddr {
    fn from(ppn: PhysPageNumber) -> Self {
        Self::from(ppn.0 << 12)
    }
}

impl From<usize> for PhysPageNumber {
    fn from(bits: usize) -> Self {
        Self(bits & ((1 << PPN_BITS) - 1))
    }
}

impl fmt::Debug for PhysPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ppn@{:x}", self.0)
    }
}

#[derive(Debug)]
pub struct PpnRange {
    ppn: PhysPageNumber,
    len: usize,
}

impl PpnRange {
    pub(crate) fn new(start: PhysPageNumber, end: PhysPageNumber) -> Option<Self> {
        if end < start {
            return None;
        }
        Some(Self {
            ppn: start,
            len: end - start,
        })
    }
}

impl IntoIterator for PpnRange {
    type Item = PhysPageNumber;

    type IntoIter = PpnRangeIter;

    fn into_iter(self) -> Self::IntoIter {
        PpnRangeIter {
            ppn: self.ppn,
            len: self.len,
        }
    }
}

pub struct PpnRangeIter {
    ppn: PhysPageNumber,
    len: usize,
}

impl Iterator for PpnRangeIter {
    type Item = PhysPageNumber;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        let ppn = self.ppn;
        self.ppn = ppn + 1;
        self.len -= 1;
        Some(ppn)
    }
}
