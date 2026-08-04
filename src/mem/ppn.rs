use core::{fmt, ops, slice};

use crate::{PAGE_ORDER, PAGE_SIZE, addr::PhysAddr};

const PPN_BITS: usize = 44;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl PhysPageNumber {
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
}

impl ops::BitXor<usize> for PhysPageNumber {
    type Output = Self;

    fn bitxor(self, rhs: usize) -> Self::Output {
        Self(self.0 ^ rhs)
    }
}

impl ops::Add<usize> for PhysPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<PhysPageNumber> for usize {
    fn from(ppn: PhysPageNumber) -> Self {
        ppn.0
    }
}

impl From<usize> for PhysPageNumber {
    fn from(bits: usize) -> Self {
        Self(bits & ((1 << PPN_BITS) - 1))
    }
}

impl From<PhysAddr> for PhysPageNumber {
    fn from(addr: PhysAddr) -> Self {
        Self(usize::from(addr) >> PAGE_ORDER)
    }
}
impl fmt::Debug for PhysPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ppn@{:x}", self.0)
    }
}

#[derive(Debug)]
pub(crate) struct PpnRange {
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
            len: usize::from(end) - usize::from(start),
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

pub(crate) struct PpnRangeIter {
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
