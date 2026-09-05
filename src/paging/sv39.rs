//! Sv39 paging scheme for 39-bit virtual addresses on 64-bit RISC-V systems.

use core::marker::PhantomData;

use crate::{
    Error,
    kalloc::Kmem,
    mem::{PAGE_SIZE, align_down, paddr::PhysAddr, ppn::PhysPageNumber, vaddr::VirtAddr},
    paging::{
        MappingError,
        page_table::{PageTable, PageTableEntry, PteFlags},
    },
    riscv::satp::Satp,
};

/// A 39-bit virtual address space.
pub struct Sv39<'t> {
    root: PhysPageNumber,
    _phantom: PhantomData<&'t mut PageTable>,
}

impl<'t> Sv39<'t> {
    /// Allocates a root page table and creates a new empty address space.
    pub fn new(kmem: &mut Kmem) -> Result<Self, Error> {
        let root = kmem.alloc(1).ok_or(Error::OutOfMemory)?;
        let root_page_table = unsafe { root.page_table_mut() };
        root_page_table.zero();
        Ok(Self {
            root,
            _phantom: PhantomData,
        })
    }

    /// Map a range of virtual addresses to a range of physical addresses.
    pub fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        flags: PteFlags,
        kmem: &mut Kmem,
    ) -> Result<(), Error> {
        if size != align_down(size, PAGE_SIZE) {
            return Err(Error::BadMapping(MappingError::BadSize(size)));
        }
        let mut vpn = vaddr.page_number();
        let mut ppn = paddr.page_number();
        if vaddr != vpn.addr() {
            return Err(Error::BadMapping(MappingError::BadAddress(vaddr)));
        }
        let end = vaddr.wrapping_add(size - 1).page_number();
        while vpn <= end {
            let indices = vpn.indices();
            let root = unsafe { self.root.page_table_mut() };
            let mut pte = &mut root[indices[2]];
            for &index_next in indices[..2].iter().rev() {
                let page_table = if pte.flags().contains(PteFlags::V) {
                    unsafe { pte.ppn().page_table_mut() }
                } else {
                    let ppn = kmem.alloc(0).ok_or(Error::OutOfMemory)?;
                    let page_table = unsafe { ppn.page_table_mut() };
                    page_table.zero();
                    *pte = pte.with_ppn(ppn).with_flags(PteFlags::V);
                    page_table
                };
                pte = &mut page_table[index_next];
            }
            if pte.flags().contains(PteFlags::V) {
                return Err(Error::BadMapping(MappingError::Remap(vaddr)));
            }
            *pte = pte.with_ppn(ppn).with_flags(flags | PteFlags::V);
            vpn = vpn + 1;
            ppn = ppn + 1;
        }
        Ok(())
    }

    // pub fn mapdirect(
    //     &mut self,
    //     addr: usize,
    //     size: usize,
    //     flags: PteFlags,
    //     kmem: &mut Kmem,
    // ) -> Result<(), Error> {
    //     self.map(VirtAddr::new(addr), PhysAddr::new(addr), size, flags, kmem)
    // }

    /// Unmap all virtual addresses and deallocate all page tables except the root.
    pub fn unmap(&mut self, kmem: &mut Kmem) {
        let root = unsafe { self.root.page_table_mut() };
        for lvl2_pte in root.iter_mut() {
            let lvl2_pte_flags = lvl2_pte.flags();
            if !lvl2_pte_flags.contains(PteFlags::V)
                || lvl2_pte_flags.intersects(PteFlags::R | PteFlags::W | PteFlags::X)
            {
                continue;
            }
            let lvl1_ppn = lvl2_pte.ppn();
            let lvl1_page_table = unsafe { lvl1_ppn.page_table_mut() };
            for lvl1_pte in lvl1_page_table.iter_mut() {
                let lvl1_pte_flags = lvl1_pte.flags();
                if !lvl1_pte_flags.contains(PteFlags::V)
                    || lvl1_pte_flags.intersects(PteFlags::R | PteFlags::W | PteFlags::X)
                {
                    continue;
                }
                let lvl0_ppn = lvl1_pte.ppn();
                *lvl1_pte = PageTableEntry::zero();
                kmem.dealloc(lvl0_ppn);
            }
            *lvl2_pte = PageTableEntry::zero();
            kmem.dealloc(lvl1_ppn);
        }
    }

    /// Translate the given virtual address to the physical address that was mapped to it.
    pub fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let root = unsafe { self.root.page_table() };
        let vpn = vaddr.page_number();
        let indices = vpn.indices();
        let mut pte = &root[indices[2]];
        for lvl in (0..3).rev() {
            let flags = pte.flags();
            if !flags.contains(PteFlags::V) {
                break;
            }
            if flags.intersects(PteFlags::R | PteFlags::W | PteFlags::X) {
                let ppn = pte.translate(vpn, lvl);
                let paddr = ppn.addr().wrapping_add(vaddr.page_offset());
                return Some(paddr);
            }
            if lvl == 0 {
                break;
            }
            let ppn = pte.ppn();
            let page_table = unsafe { ppn.page_table() };
            pte = &page_table[indices[lvl - 1]];
        }
        None
    }

    /// Get the SATP register value for this address space.
    pub const fn satp(&self) -> Satp {
        Satp::sv39(self.root)
    }
}
