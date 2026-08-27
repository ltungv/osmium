//! Types for page table nodes and entries.

use core::ops;

use bitflags::bitflags;

use crate::mem::{ppn::PhysPageNumber, vpn::VirtPageNumber};

/// A RISC-V page table node.
///
/// Under Sv39, each page table node contains 512 page table entries (PTEs)
/// and occupies exactly one 4096-byte physical page.
#[repr(C)]
#[repr(align(4096))]
#[derive(Debug)]
pub struct PageTable([PageTableEntry; 512]);

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PageTable {
    /// Creates a new empty page table. All entries are initialized to zero.
    pub const fn new() -> Self {
        Self([PageTableEntry::zero(); 512])
    }

    /// Sets all entries in the page table to zero.
    pub fn zero(&mut self) {
        for entry in self.iter_mut() {
            *entry = PageTableEntry::zero();
        }
    }

    /// Gets an immutable iterator over the page table entries.
    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.0.iter()
    }

    /// Gets a mutable iterator over the page table entries.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PageTableEntry> {
        self.0.iter_mut()
    }
}

impl ops::Index<usize> for PageTable {
    type Output = PageTableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl ops::IndexMut<usize> for PageTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

bitflags! {
    /// Flags for a RISC-V Page Table Entry (PTE).
    ///
    /// These flags control the permissions (Read, Write, Execute) and state
    /// (Valid, User, Global, Accessed, Dirty) of a memory page.
    #[derive(Clone, Copy)]
    pub struct PteFlags: usize {
        /// Valid bit.
        const V = 1 << 0;
        /// Read bit.
        const R = 1 << 1;
        /// Write bit.
        const W = 1 << 2;
        /// Execute bit.
        const X = 1 << 3;
        /// User mode bit.
        const U = 1 << 4;
        /// Global mapping bit.
        const G = 1 << 5;
        /// Accessed bit.
        const A = 1 << 6;
        /// Dirty bit.
        const D = 1 << 7;
    }
}

/// A RISC-V Page Table Entry (PTE).
///
/// A PTE contains the physical page number (PPN) of either the next level
/// page table or the actual mapped physical frame. It also contains flags
/// describing the mapping's permissions and state.
#[derive(Debug, Clone, Copy)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    /// Creates a new page table entry with all bits set to zero.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Returns the physical page number (PPN) stored in the page table entry.
    pub const fn ppn(self) -> PhysPageNumber {
        PhysPageNumber::new(self.0 >> 10)
    }

    /// Returns the permission flags stored in the page table entry.
    pub const fn flags(self) -> PteFlags {
        PteFlags::from_bits_retain(self.0 & 0xff)
    }

    /// Sets the physical page number (PPN) in the page table entry.
    pub const fn set_ppn(&mut self, ppn: PhysPageNumber) {
        let mask = ((1 << PhysPageNumber::BITS) - 1) << 10;
        self.0 &= !mask;
        self.0 |= ppn.get() << 10;
    }

    /// Sets the permission flags in the page table entry.
    pub const fn set_flags(&mut self, flags: PteFlags) {
        let mask = 0xff;
        self.0 &= !mask;
        self.0 |= flags.bits();
    }

    /// Translates the given virtual page number (VPN) to a physical page number (PPN)
    pub const fn translate(self, vpn: VirtPageNumber, lvl: usize) -> PhysPageNumber {
        let ppn = self.ppn();
        let mask = (1 << (lvl * 9)) - 1;
        let lower = vpn.get() & mask;
        let upper = ppn.get() & !mask;
        PhysPageNumber::new(upper | lower)
    }
}
