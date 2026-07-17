//! Functions and types for managing physical frames.

use core::{fmt, mem::MaybeUninit, mem::size_of, num::NonZero, slice};

use crate::mm::{PAGE_ORDER, PAGE_SIZE, PhysAddr, align_value};

/// An allocator for 4096-byte physical frames.
#[derive(Debug)]
pub struct FrameAllocator {
    descriptors: spin::Mutex<&'static mut [FrameDescriptor]>,
    alloc_start_addr: PhysAddr,
}

impl fmt::Display for FrameAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();

        let begin = PhysAddr::from(descriptors.as_ptr());
        let end = begin + size_of::<FrameDescriptor>() * descriptors.len();

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
            let is_taken = descriptor.contains(FrameDescriptorFlag::Taken);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.contains(FrameDescriptorFlag::Last);
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
    /// Creates a new frame allocator given the heap's start address and size.
    pub fn new(heap_start: NonZero<usize>, heap_size: NonZero<usize>) -> Self {
        let desc_size = size_of::<FrameDescriptor>();
        let pages = heap_size.get() / (PAGE_SIZE + desc_size);

        let alloc_start_addr = PhysAddr::from(align_value(
            heap_start.get() + pages * desc_size,
            PAGE_ORDER,
        ));

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start.get() as *mut FrameDescriptor, pages) };

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
            (offset..offset + pages).for_each(|i| descriptors[i].set(FrameDescriptorFlag::Taken));
            descriptors[offset + pages - 1].set(FrameDescriptorFlag::Last);
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
        while descriptors[page].contains(FrameDescriptorFlag::Taken)
            && !descriptors[page].contains(FrameDescriptorFlag::Last)
        {
            descriptors[page].clear();
            page += 1;
        }
        assert!(
            descriptors[page].contains(FrameDescriptorFlag::Last),
            "Possible double-free detected! (Not taken found before last)"
        );
        descriptors[page].clear();
    }

    /// find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(descriptors: &[FrameDescriptor], pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let mut current_pages_begin = None;
        for (pages_end, descriptor) in descriptors.iter().enumerate() {
            if descriptor.contains(FrameDescriptorFlag::Taken) {
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
enum FrameDescriptorFlag {
    /// page has been taken by the allocator.
    Taken = 1 << 0,

    /// page is the last one in the allocated pages.
    Last = 1 << 1,
}

#[derive(Debug)]
struct FrameDescriptor(u8);

impl FrameDescriptor {
    /// enable the bit corresponding to the given page type.
    fn set(&mut self, flag: FrameDescriptorFlag) {
        self.0 |= flag as u8;
    }

    /// return true of the given flag is set.
    fn contains(&self, flag: FrameDescriptorFlag) -> bool {
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
