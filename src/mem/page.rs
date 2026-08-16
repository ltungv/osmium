//! An implementation of the Sv39 page-based 39-bit virtual-memory system.

use core::marker::PhantomData;

use bitflags::bitflags;

use crate::{
    Error, PAGE_SIZE,
    addr::{self, PhysAddr, VirtAddr},
    mem::{frame::BuddyAlloc, ppn::PhysPageNumber, vpn::VirtPageNumber},
};

pub struct MappedPageTable<'t> {
    ppn: Option<PhysPageNumber>,
    _phantom: PhantomData<&'t mut PageTable>,
}

impl MappedPageTable<'static> {
    pub const fn empty() -> Self {
        Self {
            ppn: None,
            _phantom: PhantomData,
        }
    }

    pub unsafe fn init(&mut self, ppn: PhysPageNumber) {
        PageTable::init(ppn);
        self.ppn = Some(ppn);
    }
}

impl MappedPageTable<'_> {
    pub const fn satp(&self) -> usize {
        let ppn = if let Some(ppn) = self.ppn {
            ppn.get()
        } else {
            0
        };
        8 << 60 | ppn
    }

    pub fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let page_table = unsafe { &*PageTable::ptr_from_ppn(self.ppn?) };
        page_table.translate(vaddr)
    }

    pub fn map_range(
        &mut self,
        start: VirtAddr,
        end: VirtAddr,
        flags: PteFlags,
        allocator: &mut BuddyAlloc,
    ) -> Result<(), Error> {
        let page_table = unsafe { self.page_table().ok_or(Error::InvalidState)? };
        let vpn_start = start.align_down(PAGE_SIZE).page_number();
        let vpn_end = end.align_up(PAGE_SIZE).page_number();
        let len = vpn_end - vpn_start;
        for i in 0..len {
            let vpn = vpn_start + i;
            let ppn = PhysPageNumber::new_trunc(vpn.get());
            page_table.map(vpn, ppn, flags, 0, allocator)?;
        }
        Ok(())
    }

    unsafe fn page_table(&mut self) -> Option<&mut PageTable> {
        self.ppn
            .map(|ppn| unsafe { &mut *PageTable::ptr_mut_from_ppn(ppn) })
    }
}

#[repr(C)]
#[repr(align(4096))]
#[derive(Debug)]
struct PageTable([PageTableEntry; 4096]);

impl PageTable {
    fn map(
        &mut self,
        vpn: VirtPageNumber,
        ppn: PhysPageNumber,
        flags: PteFlags,
        lvl: usize,
        allocator: &mut BuddyAlloc,
    ) -> Result<(), Error> {
        assert!(flags.is_rwx());
        let indices = vpn.indices();
        let mut pte = &mut self.0[indices[2]];
        for &index_next in indices[lvl..2].iter().rev() {
            let page_table = Self::create(pte, allocator)?;
            pte = &mut page_table.0[index_next];
        }
        pte.set_ppn(ppn);
        pte.set_flags(flags | PteFlags::V);
        Ok(())
    }

    #[allow(dead_code)]
    fn unmap(&mut self, allocator: &mut BuddyAlloc) {
        for lvl2_pte in &mut self.0 {
            let lvl2_pte_flags = lvl2_pte.flags();
            if !lvl2_pte_flags.contains(PteFlags::V) || lvl2_pte_flags.is_rwx() {
                continue;
            }
            let lvl1_ppn = lvl2_pte.ppn();
            let lvl1_page_table = unsafe { lvl2_pte.unchecked_next_table_mut() };
            for lvl1_pte in &mut lvl1_page_table.0 {
                let lvl1_pte_flags = lvl1_pte.flags();
                if !lvl1_pte_flags.contains(PteFlags::V) || lvl1_pte_flags.is_rwx() {
                    continue;
                }
                let lvl0_ppn = lvl1_pte.ppn();
                *lvl1_pte = PageTableEntry::default();
                allocator.dealloc(lvl0_ppn);
            }
            *lvl2_pte = PageTableEntry::default();
            allocator.dealloc(lvl1_ppn);
        }
    }

    fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let vpn = vaddr.page_number();
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
                let paddr = ppn.addr().checked_add(vaddr.page_offset())?;
                return Some(paddr);
            }
            // At level 0, a valid non-leaf PTE means the table is malformed —
            // there's no deeper level to descend into.
            if lvl == 0 {
                break;
            }
            // Go to the next entry.
            let page_table = unsafe { pte.unchecked_next_table() };
            pte = &page_table.0[indices[lvl - 1]];
        }
        None
    }

    fn init(ppn: PhysPageNumber) {
        let ptr = Self::ptr_mut_from_ppn(ppn).cast::<PageTableEntry>();
        for i in 0..PAGE_SIZE / size_of::<PageTableEntry>() {
            unsafe {
                ptr.add(i).write(PageTableEntry::default());
            }
        }
    }

    fn create<'e>(
        pte: &'e mut PageTableEntry,
        allocator: &mut BuddyAlloc,
    ) -> Result<&'e mut Self, Error> {
        if !pte.flags().contains(PteFlags::V) {
            let ppn = allocator.alloc(0).ok_or(Error::OutOfMemory)?;
            Self::init(ppn);
            pte.set_ppn(ppn);
            pte.set_flags(PteFlags::V);
        }
        let page_table = unsafe { pte.unchecked_next_table_mut() };
        Ok(page_table)
    }

    #[inline]
    const fn ptr_from_ppn(ppn: PhysPageNumber) -> *const Self {
        let vaddr = unsafe { addr::phys_to_virt(ppn.addr()) };
        vaddr.as_ptr::<Self>()
    }

    #[inline]
    const fn ptr_mut_from_ppn(ppn: PhysPageNumber) -> *mut Self {
        let vaddr = unsafe { addr::phys_to_virt(ppn.addr()) };
        vaddr.as_ptr_mut::<Self>()
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
    fn is_rwx(self) -> bool {
        self.intersects(Self::R | Self::W | Self::X)
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct PageTableEntry(usize);

impl PageTableEntry {
    const fn ppn(self) -> PhysPageNumber {
        PhysPageNumber::new_trunc(self.0 >> 10)
    }

    const fn flags(self) -> PteFlags {
        PteFlags::from_bits_retain(self.0 & 0xff)
    }

    const fn translate(self, vpn: VirtPageNumber, lvl: usize) -> PhysPageNumber {
        let ppn = self.ppn();
        let mask = (1 << (lvl * 9)) - 1;
        let lower = vpn.get() & mask;
        let upper = ppn.get() & !mask;
        PhysPageNumber::new_trunc(upper | lower)
    }

    const fn set_ppn(&mut self, ppn: PhysPageNumber) {
        let mask = ((1 << PhysPageNumber::BITS) - 1) << 10;
        self.0 &= !mask;
        self.0 |= ppn.get() << 10;
    }

    const fn set_flags(&mut self, flags: PteFlags) {
        let mask = 0xff;
        self.0 &= !mask;
        self.0 |= flags.bits();
    }

    #[inline]
    const unsafe fn unchecked_next_table(&self) -> &PageTable {
        unsafe { &*PageTable::ptr_from_ppn(self.ppn()) }
    }

    #[inline]
    const unsafe fn unchecked_next_table_mut(&mut self) -> &mut PageTable {
        unsafe { &mut *PageTable::ptr_mut_from_ppn(self.ppn()) }
    }
}
