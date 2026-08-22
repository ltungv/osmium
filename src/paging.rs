//! An implementation of the Sv39 page-based 39-bit virtual-memory system.

use core::{arch::asm, marker::PhantomData};

use bitflags::bitflags;

use crate::{
    BSS_ADDR, DATA_ADDR, Error, HEAP_ADDR, MEM_ADDR, MEM_SIZE, PAGE_SIZE, RODATA_ADDR, STACK_ADDR,
    TRAMP_ADDR, UART_ADDR,
    addr::{PhysAddr, PhysPageNumber, VirtAddr, VirtPageNumber, align_down},
    kalloc::{self, BuddyAlloc},
    riscv::w_satp,
};

static KERNEL_PAGE_TABLE: spin::Mutex<MappedPageTable<'static>> =
    spin::Mutex::new(MappedPageTable::empty());

pub fn init() {
    let mut page_table = KERNEL_PAGE_TABLE.lock();
    unsafe {
        page_table.init(
            kalloc::get()
                .lock()
                .alloc(1)
                .expect("physical memory should be available"),
        );
    }
    let mut mapdirect = |addr: usize, size: usize, flags: PteFlags| {
        page_table
            .map(
                VirtAddr::new_trunc(addr),
                PhysAddr::new_trunc(addr),
                size,
                flags,
                &mut kalloc::get().lock(),
            )
            .expect("address should be mapped");
    };
    // uart memory mapped registers
    mapdirect(UART_ADDR, PAGE_SIZE, PteFlags::R | PteFlags::W);
    unsafe {
        // .text section
        mapdirect(MEM_ADDR, TRAMP_ADDR - MEM_ADDR, PteFlags::R | PteFlags::X);
        // .tramp section
        mapdirect(
            TRAMP_ADDR,
            RODATA_ADDR - TRAMP_ADDR,
            PteFlags::R | PteFlags::X,
        );
        // .rodata section
        mapdirect(
            RODATA_ADDR,
            DATA_ADDR - RODATA_ADDR,
            PteFlags::R | PteFlags::X,
        );
        // .data section
        mapdirect(DATA_ADDR, BSS_ADDR - DATA_ADDR, PteFlags::R | PteFlags::W);
        // .bss section
        mapdirect(BSS_ADDR, STACK_ADDR - BSS_ADDR, PteFlags::R | PteFlags::W);
        // kernel's stack
        mapdirect(
            STACK_ADDR,
            HEAP_ADDR - STACK_ADDR,
            PteFlags::R | PteFlags::W,
        );
        // kernel's heap
        mapdirect(
            HEAP_ADDR,
            (MEM_ADDR + MEM_SIZE) - HEAP_ADDR,
            PteFlags::R | PteFlags::W,
        );
    }
}

pub fn inithart() {
    let satp = KERNEL_PAGE_TABLE.lock().satp();
    unsafe {
        // wait for any previous writes to the page table memory to finish
        asm!("sfence.vma");
        {
            w_satp(satp);
        }
        // flush stale entries from the tlb
        asm!("sfence.vma");
    }
}

struct MappedPageTable<'t> {
    ppn: Option<PhysPageNumber>,
    _phantom: PhantomData<Option<&'t mut PageTable>>,
}

impl MappedPageTable<'static> {
    const fn empty() -> Self {
        Self {
            ppn: None,
            _phantom: PhantomData,
        }
    }
}

impl MappedPageTable<'_> {
    unsafe fn init(&mut self, ppn: PhysPageNumber) {
        PageTable::init(ppn);
        self.ppn = Some(ppn);
    }

    const fn satp(&self) -> usize {
        let ppn = if let Some(ppn) = self.ppn {
            ppn.get()
        } else {
            0
        };
        8 << 60 | ppn
    }

    fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        flags: PteFlags,
        allocator: &mut BuddyAlloc,
    ) -> Result<(), Error> {
        self.page_table_mut()
            .ok_or(Error::InvalidState)?
            .map(vaddr, paddr, size, flags, allocator)
    }

    fn unmap(&mut self, allocator: &mut BuddyAlloc) {
        let Some(page_table) = self.page_table_mut() else {
            return;
        };
        page_table.unmap(allocator);
    }

    fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        self.page_table()?.translate(vaddr)
    }

    fn page_table(&self) -> Option<&PageTable> {
        let ppn = self.ppn?;
        Some(unsafe { &*PageTable::ptr_from_ppn(ppn) })
    }

    #[expect(clippy::needless_pass_by_ref_mut)]
    fn page_table_mut(&mut self) -> Option<&mut PageTable> {
        let ppn = self.ppn?;
        Some(unsafe { &mut *PageTable::ptr_mut_from_ppn(ppn) })
    }
}

#[repr(C)]
#[repr(align(4096))]
#[derive(Debug)]
struct PageTable([PageTableEntry; 4096]);

impl PageTable {
    fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        flags: PteFlags,
        allocator: &mut BuddyAlloc,
    ) -> Result<(), Error> {
        assert_ne!(size, 0, "size should not be zero");
        assert_eq!(
            size,
            align_down(size, PAGE_SIZE),
            "size should be page-aligned"
        );
        let mut vpn = vaddr.page_number();
        let mut ppn = paddr.page_number();
        assert_eq!(vaddr, vpn.addr(), "virtual address should be page-aligned");
        let end = vaddr.wrapping_add(size).align_up(PAGE_SIZE).page_number();
        while vpn < end {
            let indices = vpn.indices();
            let mut pte = &mut self.0[indices[2]];
            for &index_next in indices[..2].iter().rev() {
                let page_table = Self::create(pte, allocator)?;
                pte = &mut page_table.0[index_next];
            }
            assert!(
                !pte.flags().contains(PteFlags::V),
                "address should not be remapped"
            );
            pte.set_ppn(ppn);
            pte.set_flags(flags | PteFlags::V);
            vpn = vpn + 1;
            ppn = ppn + 1;
        }
        Ok(())
    }

    fn unmap(&mut self, allocator: &mut BuddyAlloc) {
        for lvl2_pte in &mut self.0 {
            let lvl2_pte_flags = lvl2_pte.flags();
            if !lvl2_pte_flags.contains(PteFlags::V) || lvl2_pte_flags.is_leaf() {
                continue;
            }
            let lvl1_ppn = lvl2_pte.ppn();
            let lvl1_page_table = unsafe { lvl2_pte.unchecked_next_table_mut() };
            for lvl1_pte in &mut lvl1_page_table.0 {
                let lvl1_pte_flags = lvl1_pte.flags();
                if !lvl1_pte_flags.contains(PteFlags::V) || lvl1_pte_flags.is_leaf() {
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
            // according to risc-v, a leaf can be at any level
            if flags.is_leaf() {
                // only ppn[2:leaf-level] will be used to develop the physical address
                // if a level 2's page table entry is a leaf, only ppn[2] contribytes to the
                // physical address
                // vpn[1] is copied to ppn[1], vpn[0] is copied to ppn[0], and the page offset is
                // copied as normal
                let ppn = pte.translate(vpn, lvl);
                let paddr = ppn.addr().wrapping_add(vaddr.page_offset());
                return Some(paddr);
            }
            // at level 0, a valid non-leaf pte means the table is malformed
            if lvl == 0 {
                break;
            }
            // go to the next entry
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
        let vaddr = unsafe { VirtAddr::direct(ppn.addr()) };
        vaddr.as_ptr::<Self>()
    }

    #[inline]
    const fn ptr_mut_from_ppn(ppn: PhysPageNumber) -> *mut Self {
        let vaddr = unsafe { VirtAddr::direct(ppn.addr()) };
        vaddr.as_ptr_mut::<Self>()
    }
}

bitflags! {
    #[derive(Clone, Copy)]
    struct PteFlags: usize {
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
    fn is_leaf(self) -> bool {
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
