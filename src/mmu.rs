//! This module contains the implementation of the Memory Management Unit.

use core::{
    mem::size_of,
    sync::atomic::{self, AtomicPtr},
};

#[cfg(debug_assertions)]
use crate::println;

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

/// A page allocator that uses page descriptors to keep track of the allocations.
/// A PageDescriptor structure is allocated per `2 ^ page_order` bytes.
#[derive(Debug)]
pub struct PageAllocator {
    descriptors: AtomicPtr<Page>,
    allocations: AtomicPtr<u8>,
    total_size: usize,
    page_order: usize,
    num_pages: usize,
}

impl PageAllocator {
    /// Create a new page allocator.
    pub fn new(base_address: usize, max_size: usize, page_order: usize) -> Self {
        let page_size = 1usize << page_order;
        let desc_size = size_of::<Page>();
        let num_pages = max_size / (page_size + desc_size);
        let alloc_start = align_value(base_address + num_pages * desc_size, page_order);
        Self {
            descriptors: AtomicPtr::new(base_address as *mut Page),
            allocations: AtomicPtr::new(alloc_start as *mut u8),
            total_size: num_pages * page_size,
            page_order,
            num_pages,
        }
    }

    /// Initialize the page allocator system. There are several ways that we can
    /// implement the page allocator:
    pub fn initialize(&self) {
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        (0..self.num_pages).for_each(|i| unsafe { (*descriptors.add(i)).clear() });
    }

    /// Allocate a contiguous region of one or more pages.
    pub fn alloc(&self, pages: usize) -> Option<*mut u8> {
        assert!(pages > 0);
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        self.find_free_pages(pages).map(|offset| unsafe {
            (offset..offset + pages).for_each(|i| (*descriptors.add(i)).set_flag(PageFlag::Taken));
            (*descriptors.add(offset + pages - 1)).set_flag(PageFlag::Last);
            // The PageDescriptor structures themselves aren't the useful memory.
            // Instead, there is 1 PageDescriptor structure per 4096 bytes.
            let page_size = 1usize << self.page_order;
            allocations.add(page_size * offset)
        })
    }

    /// Allocate a contiguous region of one or more pages and set all bytes in the region to zero.
    pub fn zalloc(&self, pages: usize) -> Option<*mut u8> {
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

    /// Allocate a contiguous region starting at `ptr`.
    pub fn dealloc(&self, ptr: *const u8) {
        // Make sure we don't try to free a null pointer.
        assert!(!ptr.is_null());
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let page_size = 1usize << self.page_order;
        let page_offset = unsafe { ptr.sub(allocations as usize) as usize };
        let page = page_offset / page_size;

        let mut addr = unsafe { descriptors.add(page) };
        // Make sure that the address makes sense.
        assert!((addr as usize) < (descriptors as usize) + self.total_size);

        // Keep clearing pages until we hit the last page.
        unsafe {
            while (*addr).is_flagged(PageFlag::Taken) && !(*addr).is_flagged(PageFlag::Last) {
                (*addr).clear();
                addr = addr.add(1);
            }
        }

        // If the following assertion fails, it is most likely
        // caused by a double-free.
        assert!(
            unsafe { (*addr).is_flagged(PageFlag::Last) == true },
            "Possible double-free detected! (Not taken found before last)"
        );
        // If we get here, we've taken care of all previous pages and
        // we are on the last page.
        unsafe {
            (*addr).clear();
        }
    }

    /// Find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(&self, pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let mut current_pages_begin = None;
        for pages_end in 0..self.num_pages {
            let is_taken = unsafe { (*descriptors.add(pages_end)).is_flagged(PageFlag::Taken) };
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

    /// Print all page allocations, mainly used for debugging.
    #[cfg(debug_assertions)]
    pub fn print_page_allocations(&self) {
        let page_size = 1usize << self.page_order;
        let descriptors = self.descriptors.load(atomic::Ordering::Relaxed);
        let allocations = self.allocations.load(atomic::Ordering::Relaxed);
        let begin = descriptors;
        let alloc_begin = allocations;
        let end = unsafe { descriptors.add(self.num_pages) };
        let alloc_end = unsafe { allocations.add(self.total_size) };
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
            let descriptor = unsafe { descriptors.add(page_end) };
            let is_taken = unsafe { (*descriptor).is_flagged(PageFlag::Taken) };
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = unsafe { (*descriptor).is_flagged(PageFlag::Last) };
            if is_last {
                current_pages_begin.take();
                let addr_begin = unsafe { allocations.add(pages_begin * page_size) };
                let addr_end = unsafe { allocations.add(page_end * page_size) };
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
}

/// The page descriptor containing general information about physical memory pages.
#[derive(Debug)]
struct Page {
    flags: u8,
}

impl Page {
    /// Enable the bit corresponding to the given page type.
    fn set_flag(&mut self, flag: PageFlag) {
        self.flags |= flag.u8()
    }

    /// Return true of the given flag is set.
    fn is_flagged(&self, flag: PageFlag) -> bool {
        if self.flags == 0 && flag.u8() == 0 {
            return true;
        }
        self.flags & flag.u8() != 0
    }

    fn clear(&mut self) {
        self.flags = PageFlag::Empty.u8();
    }
}

/// All possible page type. Each type is an 8-bit bitmask.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum PageFlag {
    /// Page is not allocated.
    Empty = 0,
    /// Page is taken.
    Taken = 1 << 0,
    /// Page is that last one in a contiguous region.
    Last = 1 << 1,
}

impl PageFlag {
    /// Get the inner value of the enum.
    fn u8(self) -> u8 {
        self as u8
    }
}

/// Aligns (set to a multiple of some power of two) and always rounds up.
/// This takes an order which is the exponent to 2^order, therefore,
/// all alignments must be made as a power of two.
const fn align_value(val: usize, order: usize) -> usize {
    let o = (1usize << order) - 1;
    (val + o) & !o
}
