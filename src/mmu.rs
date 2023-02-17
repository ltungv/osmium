//! This module contains the implementation of the Memory Management Unit.

use core::{
    fmt::Display,
    mem::size_of,
    ops::{Index, IndexMut},
    sync::atomic::{self, AtomicPtr},
};

use spin::mutex::SpinMutex;

extern "C" {
    /// First memory address in the .text section
    pub static TEXT_START: usize;
    /// Last memory address in the .text section
    pub static TEXT_END: usize;
    /// First memory address in the .rodata section
    pub static RODATA_START: usize;
    /// Last memory address in the .rodata section
    pub static RODATA_END: usize;
    /// First memory address in the .data section
    pub static DATA_START: usize;
    /// Last memory address in the .data section
    pub static DATA_END: usize;
    /// First memory address in the .bss section
    pub static BSS_START: usize;
    /// Last memory address in the .bss section
    pub static BSS_END: usize;
    /// First memory address in the .kernel_stack section
    pub static KERNEL_STACK_START: usize;
    /// Last memory address in the .kernel_stack section
    pub static KERNEL_STACK_END: usize;
    /// First memory address in the .heap section
    pub static HEAP_START: usize;
    /// Last memory address in the .heap section
    pub static HEAP_SIZE: usize;
    /// First memory address
    pub static MEMORY_START: usize;
    /// Last memory address
    pub static MEMORY_END: usize;
}

/// Error occurs when working with `PageTable`.
#[derive(Debug)]
pub enum PageTableError {
    /// There's no page left.
    OutOfMemory,
    /// The kernel state is invalid
    InvalidState,
}

impl Display for PageTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "Out of memory."),
            Self::InvalidState => write!(f, "Invalid state"),
        }
    }
}

/// A 4096-byte struct containing entries that map virtual adresses to physical addresses.
#[derive(Debug)]
pub struct PageTable([PageTableEntry; 512]);

impl Default for PageTable {
    fn default() -> Self {
        PageTable([PageTableEntry(0); 512])
    }
}

impl PageTable {
    /// Create a mapping between the given virtual address and physical address.
    pub fn map(
        &mut self,
        vaddr: usize,
        paddr: usize,
        bits: i64,
        level: usize,
    ) -> Result<(), PageTableError> {
        // Make sure that Read, Write, or Execute have been provided,
        // otherwise, we'll leak memory and always create a page fault.
        assert!(bits & PageTableEntry::RWX != 0);
        let mut page_allocator = PAGE_ALLOCATOR
            .get()
            .ok_or(PageTableError::InvalidState)?
            .lock();
        let vpn_parts = [
            vaddr >> 12 & 0x1ff,
            vaddr >> 21 & 0x1ff,
            vaddr >> 30 & 0x1ff,
        ];
        // Assume the root is valid
        let mut entry = &mut self.0[vpn_parts[2]];
        for vpn_next in vpn_parts[level..2].iter().rev() {
            if !entry.is_valid() {
                // Allocate a 4096-byte page to contain to page table and mark the page entry as
                // valid. Because the page is 4096-byte aligned, we can store the page number
                // inside the page table entry instead of the entire address.
                let page = page_allocator
                    .zalloc(1)
                    .ok_or(PageTableError::OutOfMemory)?;
                // The page number can be obtain by `addr >> 12`. However, we only shift right by 2
                // because the first 10 bits are used for the flags. This means the PNN section of
                // the entry is containing the page number.
                entry.set(page as i64 >> 2 | PageTableEntry::VALID);
            }
            // Go to the next entry.
            let table = ((entry.get() & !0x3ff) << 2) as *mut PageTable;
            entry = unsafe { &mut (*table).0[*vpn_next] };
        }
        // Store the PPN at the entry at VPN[0]
        let ppn = paddr >> 12 & 0xfff_ffff_ffff;
        entry.set((ppn << 10) as i64 | bits | PageTableEntry::VALID);
        Ok(())
    }

    /// Unmap the page table.
    pub fn unmap(&mut self) -> Result<(), PageTableError> {
        let mut page_allocator = PAGE_ALLOCATOR
            .get()
            .ok_or(PageTableError::InvalidState)?
            .lock();
        for entry_lvl2 in self.0.iter() {
            if !entry_lvl2.is_valid() || entry_lvl2.is_leaf() {
                // Ignore invalid and leaf entry.
                continue;
            }
            // Get the page table.
            let table_lvl1_addr = (entry_lvl2.get() & !0x3ff) << 2;
            let table_lvl1 = {
                let table = table_lvl1_addr as *mut PageTable;
                unsafe { table.as_mut().unwrap() }
            };
            // Since the number of levels is constant, we op for nesting loops instead of recursion
            // If we recursively call `unmap` again on inner tables, we would make extraneous
            // iterations when working on the level 0 table.
            for entry_lvl1 in table_lvl1.0.iter() {
                if !entry_lvl1.is_valid() || entry_lvl1.is_leaf() {
                    // Ignore invalid and leaf entry.
                    continue;
                }
                let table_lvl0_addr = (entry_lvl1.get() & !0x3ff) << 2;
                unsafe {
                    page_allocator.dealloc(table_lvl0_addr as *const u8);
                }
            }
            unsafe {
                page_allocator.dealloc(table_lvl1_addr as *const u8);
            }
        }
        Ok(())
    }

    /// Translate the given virtual address into its corresponding physical address.
    pub fn v2p(&self, vaddr: usize) -> Option<usize> {
        let vpn_parts = [
            vaddr >> 12 & 0x1ff,
            vaddr >> 21 & 0x1ff,
            vaddr >> 30 & 0x1ff,
        ];
        // Assume the root is valid
        let mut entry = &self.0[vpn_parts[2]];
        for i in (0..3).rev() {
            if !entry.is_valid() {
                break;
            }
            if entry.is_leaf() {
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
                let offset_mask = (1 << (12 + i * 9)) - 1;
                let paddr_ls = vaddr & offset_mask;
                // The PNNs start at bit 10.
                let paddr_ms = ((entry.get() << 2) as usize) & !offset_mask;
                return Some(paddr_ms | paddr_ls);
            }
            // Go to the next entry.
            let table = ((entry.get() & !0x3ff) << 2) as *mut PageTable;
            let vpn_next = vpn_parts[i - 1];
            entry = unsafe { &mut (*table).0[vpn_next] };
        }
        None
    }

    /// Performs identity map (vaddr == paddr) for addresses in the range [start, end].
    pub fn id_map_range(
        &mut self,
        page_order: usize,
        start: usize,
        end: usize,
        bits: i64,
    ) -> Result<(), PageTableError> {
        let page_size = 1 << page_order;
        let mut addr = start & !(page_size - 1);
        let num_kb_pages = (align_value(end, page_order) - addr) / page_size;
        for _ in 0..num_kb_pages {
            self.map(addr, addr, bits, 0)?;
            addr += 1 << page_order;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct PageTableEntry(i64);

impl PageTableEntry {
    const EMPTY: i64 = 0;
    const VALID: i64 = 1 << 0;
    const READ: i64 = 1 << 1;
    const WRITE: i64 = 1 << 2;
    const EXECUTE: i64 = 1 << 3;
    const RWX: i64 = 1 << 1 | 1 << 2 | 1 << 3;

    fn get(&self) -> i64 {
        self.0
    }

    fn set(&mut self, entry: i64) {
        self.0 = entry;
    }

    // True if the V bit (bit index #0) is 1.
    fn is_valid(&self) -> bool {
        self.0 & Self::VALID != Self::EMPTY
    }

    // A leaf has one or more RWX bits set
    fn is_leaf(&self) -> bool {
        self.0 & Self::RWX != Self::EMPTY
    }
}

/// The global uart driver instance.
pub static PAGE_ALLOCATOR: spin::Once<SpinMutex<PageAllocator>> = spin::Once::new();

/// A page allocator that uses page descriptors to keep track of the allocations.
/// A PageDescriptor structure is allocated per `2 ^ page_order` bytes.
#[derive(Debug)]
pub struct PageAllocator {
    descriptors: PageDescriptors,
    allocations: AtomicPtr<u8>,
    page_order: usize,
}

impl Display for PageAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let page_size = 1usize << self.page_order;
        let total_size = self.descriptors.pages * page_size;
        //let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let begin = self.descriptors.addr();
        let end = begin + self.descriptors.pages * size_of::<PageDescriptor>();
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let alloc_begin = allocations;
        let alloc_end = unsafe { allocations.add(total_size) };
        writeln!(
            f,
            "PAGE ALLOCATION TABLE [{}/{}]",
            self.descriptors.pages, total_size,
        )?;
        writeln!(f, "META: {begin:x} -> {end:x}")?;
        writeln!(f, "PHYS: {alloc_begin:p} -> {alloc_end:p}")?;
        writeln!(f, "------------------------------------")?;
        let mut current_pages_begin = None;
        let mut count_taken = 0;
        for (page_end, descriptor) in (&self.descriptors).into_iter().enumerate() {
            let is_taken = descriptor.contains(PageDescriptor::FLAG_TAKEN);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.contains(PageDescriptor::FLAG_LAST);
            if is_last {
                current_pages_begin.take();
                let addr_begin = unsafe { allocations.add(pages_begin * page_size) };
                let addr_end = unsafe { allocations.add(page_end * page_size + page_size) };
                writeln!(
                    f,
                    "[{:>4}] 0x{:x} => 0x{:x}: {:>3} page(s)",
                    pages_begin,
                    addr_begin as usize,
                    addr_end as usize,
                    page_end - pages_begin + 1
                )?;
            }
        }
        let count_free = self.descriptors.pages - count_taken;
        if count_taken != 0 {
            writeln!(f, "------------------------------------")?;
        }
        writeln!(
            f,
            "Used: {:>6} pages ({:>10} bytes).",
            count_taken,
            count_taken * page_size
        )?;
        writeln!(
            f,
            "Free: {:>6} pages ({:>10} bytes).",
            count_free,
            count_free * page_size
        )?;
        Ok(())
    }
}

impl PageAllocator {
    /// Create a new page allocator.
    pub fn new(base_address: usize, max_size: usize, page_order: usize) -> Self {
        let page_size = 1usize << page_order;
        let desc_size = size_of::<PageDescriptor>();
        let pages = max_size / (page_size + desc_size);
        let alloc_start = align_value(base_address + pages * desc_size, page_order);
        Self {
            descriptors: PageDescriptors::new(base_address, pages),
            allocations: AtomicPtr::new(alloc_start as *mut u8),
            page_order,
        }
    }

    /// Create and initialize the global page allocator.
    pub fn initialize_global(base_address: usize, max_size: usize, page_order: usize) {
        PAGE_ALLOCATOR.call_once(|| {
            let mut allocator = Self::new(base_address, max_size, page_order);
            allocator.initialize();
            SpinMutex::new(allocator)
        });
    }

    /// Initialize the page allocator system.
    fn initialize(&mut self) {
        for descriptor in &mut self.descriptors {
            descriptor.clear();
        }
    }

    /// Allocate a contiguous region of one or more pages.
    pub fn alloc(&mut self, pages: usize) -> Option<*mut u8> {
        assert!(pages > 0);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        self.find_free_pages(pages).map(|offset| unsafe {
            (offset..offset + pages)
                .for_each(|i| self.descriptors[i].set(PageDescriptor::FLAG_TAKEN));
            self.descriptors[offset + pages - 1].set(PageDescriptor::FLAG_LAST);
            // The PageDescriptor structures themselves aren't the useful memory.
            // Instead, there is 1 PageDescriptor structure per 4096 bytes.
            let page_size = 1usize << self.page_order;
            allocations.add(page_size * offset)
        })
    }

    /// Allocate a contiguous region of one or more pages and set all bytes in the region to zero.
    pub fn zalloc(&mut self, pages: usize) -> Option<*mut u8> {
        // Allocate and zero a page.
        // First, let's get the allocation
        let page_ptr = self.alloc(pages);
        if let Some(page_ptr) = page_ptr {
            let page_size = 1usize << self.page_order;
            let big_page_ptr = page_ptr as *mut u64;
            (0..(page_size * pages) / 8).for_each(|i| unsafe { *big_page_ptr.add(i) = 0 });
        }
        page_ptr
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure that this function is only called with the starting address of a
    /// continguous page region
    pub unsafe fn dealloc(&mut self, ptr: *const u8) {
        // Make sure we don't try to free a null pointer.
        assert!(!ptr.is_null());
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let page_size = 1usize << self.page_order;
        let page_offset = ptr.sub(allocations as usize) as usize;

        let mut page = page_offset / page_size;
        while self.descriptors[page].contains(PageDescriptor::FLAG_TAKEN)
            && !self.descriptors[page].contains(PageDescriptor::FLAG_LAST)
        {
            self.descriptors[page].clear();
            page += 1;
        }
        // If the following assertion fails, it is most likely
        // caused by a double-free.
        assert!(
            self.descriptors[page].contains(PageDescriptor::FLAG_LAST),
            "Possible double-free detected! (Not taken found before last)"
        );
        // If we get here, we've taken care of all previous pages and
        // we are on the last page.
        self.descriptors[page].clear();
    }

    /// Find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(&self, pages: usize) -> Option<usize> {
        assert!(pages > 0);
        // let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let mut current_pages_begin = None;
        for (pages_end, descriptor) in self.descriptors.into_iter().enumerate() {
            let is_taken = descriptor.contains(PageDescriptor::FLAG_TAKEN);
            if is_taken {
                current_pages_begin.take();
                continue;
            }
            let pages_begin = *current_pages_begin.get_or_insert(pages_end);
            let free_pages = pages_end - pages_begin + 1;
            if free_pages == pages {
                return Some(pages_begin);
            }
        }
        None
    }
}

#[derive(Debug)]
struct PageDescriptors {
    addr: AtomicPtr<PageDescriptor>,
    pages: usize,
}

impl PageDescriptors {
    /// Create a list of page descriptors.
    fn new(base_address: usize, pages: usize) -> Self {
        Self {
            addr: AtomicPtr::new(base_address as *mut PageDescriptor),
            pages,
        }
    }

    fn addr(&self) -> usize {
        self.addr.load(atomic::Ordering::Relaxed) as usize
    }
}

impl IntoIterator for &PageDescriptors {
    type Item = &'static PageDescriptor;
    type IntoIter = PageDescriptorsIter;

    fn into_iter(self) -> Self::IntoIter {
        PageDescriptorsIter {
            addr: self.addr.load(atomic::Ordering::Relaxed),
            pages: self.pages,
            offset: 0,
        }
    }
}

impl IntoIterator for &mut PageDescriptors {
    type Item = &'static mut PageDescriptor;
    type IntoIter = PageDescriptorsIterMut;

    fn into_iter(self) -> Self::IntoIter {
        PageDescriptorsIterMut {
            addr: self.addr.load(atomic::Ordering::Relaxed),
            pages: self.pages,
            offset: 0,
        }
    }
}

impl Index<usize> for PageDescriptors {
    type Output = PageDescriptor;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.pages {
            panic!("Out of bounds");
        }
        let addr = self.addr.load(atomic::Ordering::Relaxed);
        unsafe { &*addr.add(index) }
    }
}

impl IndexMut<usize> for PageDescriptors {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if index >= self.pages {
            panic!("Out of bounds");
        }
        let addr = self.addr.load(atomic::Ordering::Relaxed);
        unsafe { &mut *addr.add(index) }
    }
}

struct PageDescriptorsIter {
    addr: *mut PageDescriptor,
    pages: usize,
    offset: usize,
}

impl Iterator for PageDescriptorsIter {
    type Item = &'static PageDescriptor;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.pages {
            return None;
        }
        let page = self.offset;
        self.offset += 1;
        unsafe { Some(&*self.addr.add(page)) }
    }
}

struct PageDescriptorsIterMut {
    addr: *mut PageDescriptor,
    pages: usize,
    offset: usize,
}

impl Iterator for PageDescriptorsIterMut {
    type Item = &'static mut PageDescriptor;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.pages {
            return None;
        }
        let page = self.offset;
        self.offset += 1;
        unsafe { Some(&mut *self.addr.add(page)) }
    }
}

/// The page descriptor containing general information about physical memory pages.
#[derive(Debug)]
struct PageDescriptor {
    flags: u8,
}

impl PageDescriptor {
    /// Page has not been used.
    const FLAG_EMPTY: u8 = 0;
    /// Page has been taken by the allocator.
    const FLAG_TAKEN: u8 = 1 << 0;
    /// Page is the last one in the allocated pages.
    const FLAG_LAST: u8 = 1 << 1;

    /// Enable the bit corresponding to the given page type.
    fn set(&mut self, flags: u8) {
        self.flags |= flags;
    }

    /// Return true of the given flag is set.
    fn contains(&self, flags: u8) -> bool {
        if self.flags == Self::FLAG_EMPTY && flags == Self::FLAG_EMPTY {
            return true;
        }
        self.flags & flags != Self::FLAG_EMPTY
    }

    /// Clear all previously set flags.
    fn clear(&mut self) {
        self.flags = Self::FLAG_EMPTY;
    }
}

/// Aligns (set to a multiple of some power of two) and always rounds up.
/// This takes an order which is the exponent to 2^order, therefore,
/// all alignments must be made as a power of two.
const fn align_value(val: usize, order: usize) -> usize {
    let o = (1usize << order) - 1;
    (val + o) & !o
}
