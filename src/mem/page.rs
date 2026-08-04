//! An implementation of the Sv39 page-based 39-bit virtual-memory system.

use core::fmt;

use bitflags::bitflags;

use crate::{
    addr::{PhysAddr, VirtAddr},
    mem::{
        frame, page,
        ppn::{PhysPageNumber, PpnRange},
        vpn::VirtPageNumber,
    },
};

/// Errors occurs when working with the page table.
#[derive(Debug)]
pub(crate) enum Error {
    /// There's no available frame.
    OutOfMemory,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

#[repr(C, align(4096))]
pub(crate) struct PageTable([PageTableEntry; 4096]);

impl Default for PageTable {
    fn default() -> Self {
        Self([PageTableEntry::default(); 4096])
    }
}

impl PageTable {
    /// Create a mapping between the given virtual address and physical address.
    pub(crate) fn map(
        &mut self,
        frame_allocator: &mut frame::Allocator,
        vpn: VirtPageNumber,
        ppn: PhysPageNumber,
        flags: PteFlags,
        lvl: usize,
    ) -> Result<(), Error> {
        assert!(flags.is_rwx());
        let indices = vpn.indices();
        let mut pte = &mut self.0[indices[2]];
        for &index_next in indices[lvl..2].iter().rev() {
            if !pte.flags().contains(PteFlags::V) {
                let inner_ppn = frame_allocator.zalloc(1).ok_or(Error::OutOfMemory)?;
                *pte = PageTableEntry::new(inner_ppn, PteFlags::V);
            }
            pte = &mut pte.ppn().as_slice_mut::<PageTableEntry>()[index_next];
        }
        *pte = PageTableEntry::new(ppn, flags | PteFlags::V);
        Ok(())
    }

    /// Unmap the page table.
    #[allow(dead_code)]
    pub(crate) fn unmap(&mut self, frame_allocator: &mut frame::Allocator) -> Result<(), Error> {
        for lvl2_pte in &mut self.0 {
            let lvl2_pte_flags = lvl2_pte.flags();
            if !lvl2_pte_flags.contains(PteFlags::V) || lvl2_pte_flags.is_rwx() {
                continue;
            }
            let lvl1_ppn = lvl2_pte.ppn();
            for lvl1_pte in lvl1_ppn.as_slice_mut::<PageTableEntry>() {
                let lvl1_pte_flags = lvl1_pte.flags();
                if !lvl1_pte_flags.contains(PteFlags::V) || lvl1_pte_flags.is_rwx() {
                    continue;
                }
                let lvl0_ppn = lvl1_pte.ppn();
                *lvl1_pte = PageTableEntry::default();
                unsafe {
                    frame_allocator.dealloc(lvl0_ppn);
                }
            }
            *lvl2_pte = PageTableEntry::default();
            unsafe {
                frame_allocator.dealloc(lvl1_ppn);
            }
        }
        Ok(())
    }

    /// Translate the given virtual address into its corresponding physical address.
    pub(crate) fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let vpn = VirtPageNumber::from(vaddr);
        let indices = vpn.indices();
        let mut pte = &self.0[indices[2]];
        for lvl in (0..3).rev() {
            let flags = pte.flags();
            if !flags.contains(PteFlags::V) {
                break;
            }
            // According to RISC-V, a leaf can be at any level.
            if flags.is_rwx() {
                // One thing to note is that only PPN[2:leaf-level] will be used to develop the
                // physical physical addres. For example, if level 2's (the top level) page table
                // entry is a leaf, then only PPN[2] contributes to the physical address. VPN[1]
                // is copied to PPN[1], VPN[0] is copied to PPN[0], and the page offset is copied,
                // as normal.
                let ppn = pte.translate(vpn, lvl);
                let paddr = PhysAddr::from(ppn) + vaddr.offset();
                return Some(paddr);
            }
            // At level 0, a valid non-leaf PTE means the table is malformed —
            // there's no deeper level to descend into.
            if lvl == 0 {
                break;
            }
            // Go to the next entry.
            pte = &pte.ppn().as_slice::<PageTableEntry>()[indices[lvl - 1]];
        }
        None
    }

    /// Performs identity map (vaddr == paddr) for addresses in the range [start, end].
    pub(crate) fn id_map_range(
        &mut self,
        frame_allocator: &mut frame::Allocator,
        start: PhysAddr,
        end: PhysAddr,
        flags: PteFlags,
    ) -> Result<(), Error> {
        let range = PpnRange::new(start.floor(), end.ceil()).unwrap();
        for ppn in range.into_iter() {
            self.map(
                frame_allocator,
                VirtPageNumber::from(usize::from(ppn)),
                ppn,
                flags,
                0,
            )?;
        }
        Ok(())
    }
}

bitflags! {
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

impl PteFlags {
    fn is_rwx(&self) -> bool {
        self.intersects(PteFlags::R | PteFlags::W | PteFlags::X)
    }
}

/// Representation of an entry in the allocation page table.
#[derive(Default, Clone, Copy)]
pub(crate) struct PageTableEntry(usize);

impl PageTableEntry {
    fn new(ppn: PhysPageNumber, flags: PteFlags) -> Self {
        Self(usize::from(ppn) << 10 | flags.bits())
    }

    fn translate(&self, vpn: VirtPageNumber, lvl: usize) -> PhysPageNumber {
        let mask = (1 << (lvl * 9)) - 1;
        let vpn = usize::from(vpn) & mask;
        let ppn = usize::from(self.ppn()) & !mask;
        PhysPageNumber::from(ppn | vpn)
    }

    fn ppn(self) -> PhysPageNumber {
        PhysPageNumber::from(self.0 >> 10)
    }

    fn flags(&self) -> PteFlags {
        PteFlags::from_bits_retain(self.0 & 0xff)
    }
}

pub struct Mapper<'a> {
    lvl2_page_table: &'a mut PageTable,
}

impl<'a> Mapper<'a> {
    pub(crate) fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        todo!()
    }

    pub(crate) fn map(
        &mut self,
        vpn: VirtPageNumber,
        ppn: PhysPageNumber,
        flags: PteFlags,
        allocator: &frame::Allocator,
    ) -> Result<(), page::Error> {
        todo!()
    }

    pub(crate) fn map_identity(
        &mut self,
        addr: usize,
        flags: PteFlags,
        allocator: &frame::Allocator,
    ) -> Result<(), page::Error> {
        todo!()
    }

    pub(crate) fn unmap(&mut self) -> Result<(), page::Error> {
        todo!()
    }
}
