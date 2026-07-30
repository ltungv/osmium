use core::{fmt, ops, slice};

use crate::mem::{PAGE_SIZE, PAGE_SIZE_BITS, addr::PhysAddress};

const PPN_BITS: usize = 44;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysPageNumber(usize);

impl fmt::Debug for PhysPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PPN(0x{:x})", self.0)
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

impl From<PhysAddress> for PhysPageNumber {
    fn from(addr: PhysAddress) -> Self {
        Self(usize::from(addr) >> PAGE_SIZE_BITS)
    }
}

impl PhysPageNumber {
    pub(crate) fn as_slice<T>(self) -> &'static [T] {
        let physical_address = PhysAddress::from(self);
        unsafe {
            slice::from_raw_parts(
                physical_address.as_ptr().cast::<T>(),
                PAGE_SIZE / size_of::<T>(),
            )
        }
    }

    pub(crate) fn as_slice_mut<T>(self) -> &'static mut [T] {
        let physical_address = PhysAddress::from(self);
        unsafe {
            slice::from_raw_parts_mut(
                physical_address.as_ptr_mut().cast::<T>(),
                PAGE_SIZE / size_of::<T>(),
            )
        }
    }
}

#[derive(Debug)]
pub(crate) struct PpnRange {
    ppn: PhysPageNumber,
    len: usize,
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

impl PpnRange {
    pub(crate) fn new(start: PhysPageNumber, end: PhysPageNumber) -> Self {
        assert!(end >= start);
        let len = usize::from(end) - usize::from(start);
        Self { ppn: start, len }
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
        self.ppn = self.ppn + 1;
        self.len -= 1;
        Some(ppn)
    }
}
