//! This module contains the implementation of the Memory Management Unit.

/// All possible page type. Each type is an 8-bit bitmask.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PageType {
    /// Page is not allocated.
    Empty = 0,
    /// Page is taken.
    Taken = 1 << 0,
    /// Page is that last one in a contiguous region.
    Last = 1 << 1,
}

impl PageType {
    /// Get the inner value of the enum.
    pub fn repr(self) -> u8 {
        self as u8
    }
}

/// An 8-bit bitmask representing to page type.
#[derive(Debug)]
pub struct PageFlag(u8);

impl Default for PageFlag {
    fn default() -> Self {
        Self(PageType::Empty.repr())
    }
}

impl PageFlag {
    /// Enable the bit corresponding to the given page type.
    pub fn on(&mut self, page_type: PageType) {
        self.0 |= page_type.repr()
    }

    /// Disable the bit corresponding to the given page type.
    pub fn off(&mut self, page_type: PageType) {
        self.0 &= !(page_type.repr())
    }
}

/// The page descriptor containing general information about physical memory pages.
#[derive(Debug)]
pub struct PageDescriptor {
    flag: PageFlag,
}
