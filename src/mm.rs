//! Memory management.

use core::{fmt, mem::MaybeUninit, mem::size_of, num::NonZero, ops::Add, slice};

use crate::mm::{self};

const PAGE_ORDER: usize = 12;

const PAGE_SIZE: usize = 1 << PAGE_ORDER;

unsafe extern "C" {
    /// First memory address in the .text section.
    pub static TEXT_START: usize;

    /// Last memory address in the .text section.
    pub static TEXT_END: usize;

    /// First memory address in the .rodata section.
    pub static RODATA_START: usize;

    /// Last memory address in the .rodata section.
    pub static RODATA_END: usize;

    /// First memory address in the .data section.
    pub static DATA_START: usize;

    /// Last memory address in the .data section.
    pub static DATA_END: usize;

    /// First memory address in the .bss section.
    pub static BSS_START: usize;

    /// Last memory address in the .bss section.
    pub static BSS_END: usize;

    /// First memory address of the kernel's stack.
    pub static KERNEL_STACK_START: usize;

    /// Last memory address of the kernel's stack.
    pub static KERNEL_STACK_END: usize;

    /// First memory address of the heap.
    pub static HEAP_START: usize;

    /// Size of the heap in bytes.
    pub static HEAP_SIZE: usize;

    /// First memory address.
    pub static MEMORY_START: usize;

    /// Last memory address.
    pub static MEMORY_END: usize;
}

static FRAME_ALLOCATOR: spin::Once<FrameAllocator> = spin::Once::new();

/// Grabs the physical frame allocator.
pub fn frame_allocator() -> &'static FrameAllocator {
    FRAME_ALLOCATOR.call_once(|| {
        let heap_start = unsafe { NonZero::new(mm::HEAP_START) };
        let heap_size = unsafe { NonZero::new(mm::HEAP_SIZE) };
        FrameAllocator::new(
            heap_start.expect("non-zero heap start"),
            heap_size.expect("non-zero heap size"),
        )
    })
}

/// Metadata for a region of byte-level allocation.
#[derive(Debug)]
pub struct AllocationNode(usize);

impl AllocationNode {
    /// Flag the current node as being taken.
    pub const FLAG_TAKEN: usize = 1 << 63;

    /// Return true if the node is taken.
    pub fn is_taken(&self) -> bool {
        self.0 & Self::FLAG_TAKEN != 0
    }

    /// Return true if the node is free.
    pub fn is_free(&self) -> bool {
        !self.is_taken()
    }

    /// Flag the node as being taken.
    pub fn take(&mut self) {
        self.0 |= Self::FLAG_TAKEN;
    }

    /// Clear the taken flag.
    pub fn free(&mut self) {
        self.0 &= !Self::FLAG_TAKEN;
    }

    /// Set the node size
    pub fn set_size(&mut self, size: usize) {
        let is_taken = self.is_taken();
        self.0 = size & !Self::FLAG_TAKEN;
        if is_taken {
            self.0 |= Self::FLAG_TAKEN;
        }
    }

    /// Get the node size
    pub fn get_size(&self) -> usize {
        self.0 & !Self::FLAG_TAKEN
    }
}

/// Errors occurs when working with the page table.
#[derive(Debug)]
pub enum PageTableError {
    /// There's no free memory page left.
    OutOfMemory,

    /// The page table is in an invalid state.
    InvalidState,
}

impl fmt::Display for PageTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "out of memory."),
            Self::InvalidState => write!(f, "invalid state"),
        }
    }
}

/// A 4096-byte struct containing entries that map virtual adresses to physical addresses.
#[derive(Debug)]
#[repr(C, align(4096))]
pub struct Sv39PageTable([Sv39PageTableEntry; 512]);

impl Sv39PageTable {
    /// Create a mapping between the given virtual address and physical address.
    pub fn map(
        &mut self,
        page_allocator: &FrameAllocator,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        flags: Sv39PageTableEntryFlags,
        level: usize,
    ) -> Result<(), PageTableError> {
        // Make sure the read, write, and execute flags have been provided. Otherwise, we'll leak
        // memory and always create a page fault.
        assert!(flags.is_readable() || flags.is_writeable() || flags.is_executable());

        // Extract the virtual page numbers from the virtual address.
        let vpns = vaddr.vpns();

        // Assume the root page table is valid
        let mut entry = &mut self.0[vpns[2]];
        for vpn_next in vpns[level..2].iter().rev() {
            if !entry.get_flags().is_valid() {
                // Allocate a 4096-byte page to contain to page table and mark the page entry as
                // valid. Because every page is 4096-byte aligned, only the physical page number
                // needs to be stored instead of the entire address.
                let page = page_allocator
                    .zalloc(1)
                    .ok_or(PageTableError::OutOfMemory)?;

                *entry = entry
                    .set_address(page)
                    .set_flags(Sv39PageTableEntryFlags::default().set_valid(true));
            }

            // Go to the next entry.
            let table = entry.get_address() as *mut Sv39PageTable;
            entry = unsafe { &mut (*table).0[*vpn_next] };
        }

        *entry = entry.set_address(paddr).set_flags(flags.set_valid(true));

        Ok(())
    }

    /// Unmap the page table.
    pub fn unmap(&mut self, page_allocator: &mut FrameAllocator) -> Result<(), PageTableError> {
        for entry_lvl2 in self.0.iter() {
            let entry_lvl2_flags = entry_lvl2.get_flags();
            if !entry_lvl2_flags.is_valid() || entry_lvl2_flags.is_leaf() {
                // Ignore invalid and leaf entry.
                continue;
            }
            // Get the page table.
            let table_lvl1_addr = entry_lvl2.get_address();
            let table_lvl1 = {
                let table = table_lvl1_addr as *mut Sv39PageTable;
                unsafe { table.as_mut().unwrap() }
            };
            // Since the number of levels is constant, we op for nesting loops instead of recursion
            // If we recursively call `unmap` again on inner tables, we would make extraneous
            // iterations when working on the level 0 table.
            for entry_lvl1 in table_lvl1.0.iter() {
                let entry_lvl1_flags = entry_lvl1.get_flags();
                if !entry_lvl1_flags.is_valid() || entry_lvl1_flags.is_leaf() {
                    // Ignore invalid and leaf entry.
                    continue;
                }
                let table_lvl0_addr = entry_lvl1.get_address();
                unsafe {
                    page_allocator.dealloc(PhysAddr::from(table_lvl0_addr));
                }
            }
            unsafe {
                page_allocator.dealloc(PhysAddr::from(table_lvl1_addr));
            }
        }
        Ok(())
    }

    /// Translate the given virtual address into its corresponding physical address.
    pub fn v2p(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        // Extract the virtual page numbers from the virtual address.
        let vpn_parts = vaddr.vpns();

        // Assume the root is valid
        let mut entry = &self.0[vpn_parts[2]];
        for i in (0..3).rev() {
            let flags = entry.get_flags();
            if !flags.is_valid() {
                break;
            }
            if flags.is_leaf() {
                // According to RISC-V, a leaf can be at any level.
                //
                // One thing to note is that only PPN[2:leaf-level] will be used to develop the
                // physical physical addres. For example, if level 2's (the top level) page table
                // entry is a leaf, then only PPN[2] contributes to the physical address. VPN[1]
                // is copied to PPN[1], VPN[0] is copied to PPN[0], and the page offset is copied,
                // as normal.
                //
                // The offset mask masks off the PPN. Each PPN is 9 bits and they start
                // at bit #12. So, our formula 12 + i * 9
                return Some(entry.translate(vaddr, i));
            }
            // Go to the next entry.
            let table = entry.get_address() as *mut Sv39PageTable;
            let vpn_next = vpn_parts[i - 1];
            entry = unsafe { &mut (*table).0[vpn_next] };
        }
        None
    }

    /// Performs identity map (vaddr == paddr) for addresses in the range [start, end].
    pub fn id_map_range(
        &mut self,
        page_allocator: &mut FrameAllocator,
        start: usize,
        end: usize,
        flags: Sv39PageTableEntryFlags,
    ) -> Result<(), PageTableError> {
        let mut addr = start & !(PAGE_SIZE - 1);
        let num_kb_pages = (align_value(end, PAGE_ORDER) - addr) / PAGE_SIZE;
        for _ in 0..num_kb_pages {
            self.map(page_allocator, addr.into(), addr.into(), flags, 0)?;
            addr += PAGE_SIZE;
        }
        Ok(())
    }
}

impl Default for Sv39PageTable {
    fn default() -> Self {
        Self([Sv39PageTableEntry(0); 512])
    }
}

/// A page table entry as described in RISC-V Sv39's specifications.
#[derive(Debug, Default, Clone, Copy)]
pub struct Sv39PageTableEntryFlags(u8);

impl Sv39PageTableEntryFlags {
    const V_BIT: u8 = 1 << 0;
    const R_BIT: u8 = 1 << 1;
    const W_BIT: u8 = 1 << 2;
    const E_BIT: u8 = 1 << 3;
    const U_BIT: u8 = 1 << 4;
    const G_BIT: u8 = 1 << 5;
    const A_BIT: u8 = 1 << 6;
    const D_BIT: u8 = 1 << 7;

    fn is_valid(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::V_BIT)
    }

    fn is_readable(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::R_BIT)
    }

    fn is_writeable(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::W_BIT)
    }

    fn is_executable(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::E_BIT)
    }

    fn is_user_mode(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::U_BIT)
    }

    fn is_global_mapping(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::G_BIT)
    }

    fn is_accessed(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::A_BIT)
    }

    fn is_dirty(&self) -> bool {
        self.is_set(Sv39PageTableEntryFlags::D_BIT)
    }

    fn is_leaf(&self) -> bool {
        self.is_readable() | self.is_writeable() | self.is_executable()
    }

    fn is_set(&self, bits: u8) -> bool {
        self.0 & bits != 0
    }

    fn set_valid(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::V_BIT, v)
    }

    fn set_readable(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::R_BIT, v)
    }

    fn set_writeable(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::W_BIT, v)
    }

    fn set_executable(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::E_BIT, v)
    }

    fn set_user_mode(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::U_BIT, v)
    }

    fn set_global_mapping(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::G_BIT, v)
    }

    fn set_accessed(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::A_BIT, v)
    }

    fn set_dirty(self, v: bool) -> Self {
        self.set(Sv39PageTableEntryFlags::D_BIT, v)
    }

    fn set(self, bits: u8, v: bool) -> Self {
        if v {
            Self(self.0 | bits)
        } else {
            Self(self.0 & !bits)
        }
    }
}

/// Representation of an entry in the allocation page table.
#[derive(Debug, Clone, Copy)]
pub struct Sv39PageTableEntry(usize);

impl Sv39PageTableEntry {
    fn translate(&self, vaddr: VirtAddr, lvl: usize) -> PhysAddr {
        let offset_mask = (1 << (12 + lvl * 9)) - 1;
        let offset = vaddr.0 & offset_mask;
        let ppns = self.get_address() & !offset_mask;
        PhysAddr(ppns | offset)
    }

    fn get_address(&self) -> usize {
        (self.0 & !0x3ff) << 2
    }

    fn set_address(self, addr: PhysAddr) -> Self {
        let ppns = addr.ppns();
        Self(self.0 | (ppns[2]) << 28 | (ppns[1]) << 19 | (ppns[0]) << 10)
    }

    fn get_flags(&self) -> Sv39PageTableEntryFlags {
        Sv39PageTableEntryFlags((self.0 & 0xff) as u8)
    }

    fn set_flags(self, flags: Sv39PageTableEntryFlags) -> Self {
        Self(self.0 | flags.0 as usize)
    }
}

/// An allocator for 4096-byte physical frames.
#[derive(Debug)]
pub struct FrameAllocator {
    descriptors: spin::Mutex<&'static mut [PageDescriptor]>,
    alloc_start_addr: PhysAddr,
}

impl fmt::Display for FrameAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();

        let begin = PhysAddr::from(descriptors.as_ptr());
        let end = begin + size_of::<PageDescriptor>() * descriptors.len();

        let alloc_total_size = descriptors.len() * PAGE_SIZE;
        let alloc_begin = self.alloc_start_addr;
        let alloc_end = alloc_begin + alloc_total_size;

        writeln!(f, "------------------------------------")?;
        writeln!(
            f,
            "PageAllocator [pages={} size={}]",
            descriptors.len(),
            alloc_total_size,
        )?;
        writeln!(f, "desc: {begin} -> {end}")?;
        writeln!(f, "phys: {alloc_begin} -> {alloc_end}")?;
        let mut current_pages_begin = None;
        let mut count_taken = 0;
        for (page_end, descriptor) in descriptors.iter().enumerate() {
            let is_taken = descriptor.contains(PageDescriptorFlag::Taken);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.contains(PageDescriptorFlag::Last);
            if is_last {
                current_pages_begin.take();
                let addr_begin = self.alloc_start_addr + pages_begin * PAGE_SIZE;
                let addr_end = self.alloc_start_addr + page_end * PAGE_SIZE;
                writeln!(
                    f,
                    "[{:>4}] {} => {}: {:>3} page(s)",
                    pages_begin,
                    addr_begin,
                    addr_end,
                    page_end - pages_begin + 1
                )?;
            }
        }
        let count_free = descriptors.len() - count_taken;
        if count_taken != 0 {
            writeln!(f, "------------------------------------")?;
        }
        writeln!(
            f,
            "used: {:>6} pages ({:>10} bytes).",
            count_taken,
            count_taken * PAGE_SIZE
        )?;
        writeln!(
            f,
            "free: {:>6} pages ({:>10} bytes).",
            count_free,
            count_free * PAGE_SIZE
        )?;
        writeln!(f, "------------------------------------")?;
        Ok(())
    }
}

impl FrameAllocator {
    fn new(heap_start: NonZero<usize>, heap_size: NonZero<usize>) -> Self {
        let desc_size = size_of::<PageDescriptor>();
        let pages = heap_size.get() / (PAGE_SIZE + desc_size);

        let alloc_start_addr = PhysAddr::from(align_value(
            heap_start.get() + pages * desc_size,
            PAGE_ORDER,
        ));

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start.get() as *mut PageDescriptor, pages) };

        for descriptor in descriptors.iter_mut() {
            descriptor.clear();
        }

        Self {
            alloc_start_addr,
            descriptors: spin::Mutex::new(descriptors),
        }
    }

    /// Allocates a contiguous region of `pages` and returns the address at the start of the region.
    /// If there's not enough memory, returns `None`.
    pub fn alloc(&self, pages: usize) -> Option<PhysAddr> {
        assert!(pages > 0);
        let mut descriptors = self.descriptors.lock();
        Self::find_free_pages(&descriptors, pages).map(|offset| {
            (offset..offset + pages).for_each(|i| descriptors[i].set(PageDescriptorFlag::Taken));
            descriptors[offset + pages - 1].set(PageDescriptorFlag::Last);
            self.alloc_start_addr + PAGE_SIZE * offset
        })
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub fn zalloc(&self, pages: usize) -> Option<PhysAddr> {
        let addr = self.alloc(pages);
        if let Some(addr) = addr {
            let qwords = unsafe {
                slice::from_raw_parts_mut(
                    addr.as_ptr_mut::<MaybeUninit<u64>>(),
                    (PAGE_SIZE * pages) / 8,
                )
            };
            for qword in qwords {
                qword.write(0);
            }
        }
        addr
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure that this function is only called with the starting address of a
    /// continguous page region
    pub unsafe fn dealloc(&self, ptr: PhysAddr) {
        assert!(ptr != PhysAddr::ZERO);
        let page_offset = ptr.offset_from(self.alloc_start_addr) as usize;
        let mut page = page_offset / PAGE_SIZE;
        let mut descriptors = self.descriptors.lock();
        while descriptors[page].contains(PageDescriptorFlag::Taken)
            && !descriptors[page].contains(PageDescriptorFlag::Last)
        {
            descriptors[page].clear();
            page += 1;
        }
        assert!(
            descriptors[page].contains(PageDescriptorFlag::Last),
            "Possible double-free detected! (Not taken found before last)"
        );
        descriptors[page].clear();
    }

    /// find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(descriptors: &[PageDescriptor], pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let mut current_pages_begin = None;
        for (pages_end, descriptor) in descriptors.iter().enumerate() {
            if descriptor.contains(PageDescriptorFlag::Taken) {
                current_pages_begin.take();
                continue;
            }
            let pages_begin = *current_pages_begin.get_or_insert(pages_end);
            if pages_end - pages_begin + 1 == pages {
                return Some(pages_begin);
            }
        }
        None
    }
}

#[derive(Debug)]
enum PageDescriptorFlag {
    /// page has been taken by the allocator.
    Taken = 1 << 0,

    /// page is the last one in the allocated pages.
    Last = 1 << 1,
}

#[derive(Debug)]
struct PageDescriptor(u8);

impl PageDescriptor {
    /// enable the bit corresponding to the given page type.
    fn set(&mut self, flag: PageDescriptorFlag) {
        self.0 |= flag as u8;
    }

    /// return true of the given flag is set.
    fn contains(&self, flag: PageDescriptorFlag) -> bool {
        if self.0 == 0 {
            return false;
        }
        self.0 & flag as u8 != 0
    }

    /// clear all previously set flags.
    fn clear(&mut self) {
        self.0 = 0;
    }
}

const fn align_value(val: usize, order: usize) -> usize {
    assert!(order > 0);
    let o = (1usize << order) - 1;
    (val + o) & !o
}

/// A physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(usize);

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Phys({:x})", self.0)
    }
}

impl<T> From<*const T> for PhysAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for PhysAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl PhysAddr {
    /// The zero physical address.
    pub const ZERO: Self = Self(0);

    /// Decomposes the physical address into physical page numbers (PPNs).
    pub fn ppns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x1ff,
        ]
    }

    /// Offset in bytes between this physical address and another one.
    pub fn offset_from(self, other: Self) -> isize {
        self.0 as isize - other.0 as isize
    }

    /// Derives a raw pointer from this address.
    pub fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

/// A virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(usize);

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Virt({:x})", self.0)
    }
}

impl<T> From<*const T> for VirtAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl VirtAddr {
    /// The zero virtual address.
    pub const ZERO: Self = Self(0);

    /// Decompose the virtual address into virtual page numbers (VPNs).
    pub fn vpns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x1ff,
        ]
    }
}
