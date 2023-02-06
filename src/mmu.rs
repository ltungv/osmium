//! This module contains the implementation of the Memory Management Unit.

use core::{
    mem::size_of,
    sync::atomic::{self, AtomicPtr},
};

use crate::println;

/// All possible page type. Each type is an 8-bit bitmask.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PageFlag {
    /// Page is not allocated.
    Empty = 0,
    /// Page is taken.
    Taken = 1 << 0,
    /// Page is that last one in a contiguous region.
    Last = 1 << 1,
}

impl PageFlag {
    /// Get the inner value of the enum.
    pub fn repr(self) -> u8 {
        self as u8
    }
}

/// The page descriptor containing general information about physical memory pages.
#[derive(Debug)]
pub struct PageDescriptor {
    flags: u8,
}

impl PageDescriptor {
    fn clear(&mut self) {
        self.flags = PageFlag::Empty.repr();
    }

    /// Enable the bit corresponding to the given page type.
    pub fn set_flag(&mut self, flag: PageFlag) {
        self.flags |= flag.repr()
    }

    /// Disable the bit corresponding to the given page type.
    pub fn clear_flag(&mut self, flag: PageFlag) {
        self.flags &= !(flag.repr())
    }

    /// Return true of the given flag is set.
    pub fn is_flagged(&self, flag: PageFlag) -> bool {
        if self.flags == 0 && flag.repr() == 0 {
            return true;
        }
        self.flags & flag.repr() != 0
    }
}

/// A page allocator that uses page descriptors to keep track of the allocations.
/// A PageDescriptor structure is allocated per `2 ^ page_order` bytes.
#[derive(Debug)]
pub struct PageAllocator {
    descriptors: AtomicPtr<PageDescriptor>,
    allocations: AtomicPtr<u8>,
    total_size: usize,
    page_order: usize,
    num_pages: usize,
}

impl PageAllocator {
    /// Create a new page allocator.
    pub fn new(base_address: usize, max_size: usize, page_order: usize) -> Self {
        let page_size = 1usize << page_order;
        let desc_size = size_of::<PageDescriptor>();
        let num_pages = max_size / (page_size + desc_size);
        let alloc_start = align_value(base_address + num_pages * desc_size, page_order);
        Self {
            descriptors: AtomicPtr::new(base_address as *mut PageDescriptor),
            allocations: AtomicPtr::new(alloc_start as *mut u8),
            total_size: num_pages * page_size,
            page_order,
            num_pages,
        }
    }

    /// Initialize the page allocator system. There are several ways that we can
    /// implement the page allocator:
    pub unsafe fn initialize(&self) {
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        (0..self.num_pages).for_each(|i| (*descriptors.add(i)).clear());
    }

    /// Allocate a contiguous region of one or more pages.
    pub unsafe fn alloc(&self, pages: usize) -> Option<*mut u8> {
        assert!(pages > 0);
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        self.find_free_pages(pages).map(|offset| {
            (offset..offset + pages).for_each(|i| (*descriptors.add(i)).set_flag(PageFlag::Taken));
            (*descriptors.add(offset + pages - 1)).set_flag(PageFlag::Last);
            // The PageDescriptor structures themselves aren't the useful memory.
            // Instead, there is 1 PageDescriptor structure per 4096 bytes.
            let page_size = 1usize << self.page_order;
            allocations.add(page_size * offset)
        })
    }

    /// Allocate a contiguous region of one or more pages and set all bytes in the region to zero.
    pub unsafe fn zalloc(&self, pages: usize) -> Option<*mut u8> {
        // Allocate and zero a page.
        // First, let's get the allocation
        let page_ptr = self.alloc(pages);
        if let Some(page_ptr) = page_ptr {
            let page_size = 1usize << self.page_order;
            let big_page_ptr = page_ptr as *mut u64;
            (0..(page_size * pages) / 8).for_each(|i| *big_page_ptr.add(i) = 0);
        }
        page_ptr
    }

    /// Allocate a contiguous region starting at `ptr`.
    pub unsafe fn dealloc(&self, ptr: *const u8) {
        // Make sure we don't try to free a null pointer.
        assert!(!ptr.is_null());
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let page_size = 1usize << self.page_order;
        let page = ptr.sub(allocations as usize) as usize / page_size;

        let mut addr = descriptors.add(page);
        // Make sure that the address makes sense.
        assert!((addr as usize) < (descriptors as usize) + self.total_size);
        // Keep clearing pages until we hit the last page.
        while (*addr).is_flagged(PageFlag::Taken) && !(*addr).is_flagged(PageFlag::Last) {
            (*addr).clear();
            addr = addr.add(1);
        }
        // If the following assertion fails, it is most likely
        // caused by a double-free.
        assert!(
            (*addr).is_flagged(PageFlag::Last) == true,
            "Possible double-free detected! (Not taken found before last)"
        );
        // If we get here, we've taken care of all previous pages and
        // we are on the last page.
        (*addr).clear();
    }

    /// Print all page allocations, mainly used for debugging.
    pub unsafe fn print_page_allocations(&self) {
        let page_size = 1usize << self.page_order;
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let begin = descriptors;
        let end = descriptors.add(self.num_pages);
        let alloc_begin = allocations;
        let alloc_end = allocations.add(self.total_size);
        println!();
        println!(
            "PAGE ALLOCATION TABLE [{}/{}]",
            self.num_pages, self.total_size,
        );
        println!("META: {:p} -> {:p}", begin, end);
        println!("PHYS: {:p} -> {:p}", alloc_begin, alloc_end);
        println!("------------------------------------");
        let mut current_pages_begin = None;
        let mut count_taken = 0;
        for page_end in 0..self.num_pages {
            let descriptor = descriptors.add(page_end);
            let is_taken = (*descriptor).is_flagged(PageFlag::Taken);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = (*descriptor).is_flagged(PageFlag::Last);
            if is_last {
                current_pages_begin.take();
                let addr_begin = allocations.add(pages_begin * page_size);
                let addr_end = allocations.add(page_end * page_size);
                println!(
                    "[{:>4}] 0x{:x} => 0x{:x}: {:>3} page(s)",
                    pages_begin,
                    addr_begin as usize,
                    addr_end as usize,
                    page_end - pages_begin + 1
                );
            }
        }
        let count_free = self.num_pages - count_taken;
        if count_taken != 0 {
            println!("------------------------------------");
        }
        println!(
            "Used: {:>6} pages ({:>10} bytes).",
            count_taken,
            count_taken * page_size
        );
        println!(
            "Free: {:>6} pages ({:>10} bytes).",
            count_free,
            count_free * page_size
        );
        println!();
    }

    /// Find a first address of a contiguous region of one or more free pages.
    unsafe fn find_free_pages(&self, pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let mut current_pages_begin = None;
        for pages_end in 0..self.num_pages {
            let is_taken = (*descriptors.add(pages_end)).is_flagged(PageFlag::Taken);
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

/// Aligns (set to a multiple of some power of two) and always rounds up.
/// This takes an order which is the exponent to 2^order, therefore,
/// all alignments must be made as a power of two.
const fn align_value(val: usize, order: usize) -> usize {
    let o = (1usize << order) - 1;
    (val + o) & !o
}
