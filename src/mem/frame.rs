//! Functions and types for managing physical frames.

use core::{fmt, mem::size_of, slice};

use bitflags::bitflags;

use crate::mem::{PAGE_SIZE, addr::PhysAddress, ppn::PhysPageNumber};

/// An allocator for 4096-byte physical frames.
pub(crate) struct Allocator {
    base_ppn: PhysPageNumber,
    descriptors: spin::Mutex<&'static mut [FrameDescriptor]>,
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();

        let desc_total_size = descriptors.len() * size_of::<FrameDescriptor>();
        let desc_begin = PhysAddress::from(descriptors.as_ptr() as usize);
        let desc_end = desc_begin + desc_total_size;

        let alloc_total_size = descriptors.len() * PAGE_SIZE;
        let alloc_begin = PhysAddress::from(self.base_ppn);
        let alloc_end = alloc_begin + alloc_total_size;

        writeln!(f, "------------------------------------")?;
        writeln!(
            f,
            "PageAllocator [pages={} size={}]",
            descriptors.len(),
            alloc_total_size,
        )?;
        writeln!(f, "desc: {:?} -> {:?}", desc_begin, desc_end)?;
        writeln!(f, "phys: {:?} -> {:?}", alloc_begin, alloc_end)?;
        writeln!(f, "------------------------------------")?;
        let mut current_pages_begin = None;
        let mut count_taken = 0;
        for (page_end, descriptor) in descriptors.iter().enumerate() {
            let is_taken = descriptor.has(FrameDescriptorFlags::TAKEN);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.has(FrameDescriptorFlags::LAST);
            if is_last {
                current_pages_begin.take();
                let alloc_begin = self.base_ppn + pages_begin;
                let alloc_end = self.base_ppn + page_end;
                writeln!(
                    f,
                    "[{:>4}] {:?} -> {:?}: {:>3} page(s)",
                    pages_begin,
                    alloc_begin,
                    alloc_end,
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
        write!(f, "------------------------------------")?;
        Ok(())
    }
}

impl Allocator {
    /// Creates a new frame allocator given the heap's start address and size.
    ///
    /// # Safety
    ///
    /// Caller must guarantee that the memory region from `heap_start` to `heap_start + heap_size`
    /// is physically available for this allocator to manage.
    pub(crate) unsafe fn new(heap_start: usize, heap_size: usize) -> Self {
        let pages = heap_size / (size_of::<FrameDescriptor>() + PAGE_SIZE);
        let base_ppn = PhysAddress::from(heap_start + size_of::<FrameDescriptor>() * pages).ceil();

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start as *mut FrameDescriptor, pages) };

        for descriptor in descriptors.iter_mut() {
            descriptor.clear();
        }

        Self {
            base_ppn,
            descriptors: spin::Mutex::new(descriptors),
        }
    }

    /// Allocates a contiguous region of `pages` and returns the address at the start of the region.
    /// If there's not enough memory, returns `None`.
    pub(crate) fn alloc(&self, pages: usize) -> Option<PhysPageNumber> {
        let mut descriptors = self.descriptors.lock();
        let offset = Self::find_free_pages(&descriptors, pages)?;
        descriptors[offset + pages - 1].set(FrameDescriptorFlags::LAST);
        for i in offset..offset + pages {
            descriptors[i].set(FrameDescriptorFlags::TAKEN);
        }
        Some(self.base_ppn + offset)
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub(crate) fn zalloc(&self, pages: usize) -> Option<PhysPageNumber> {
        self.alloc(pages)
            .inspect(|frame| frame.as_slice_mut::<u8>().fill(0))
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure this function is only called with the starting address of a contiguous
    /// page region that was previously allocated by this frame allocator.
    pub(crate) unsafe fn dealloc(&self, ppn: PhysPageNumber) {
        assert!(ppn >= self.base_ppn);
        let mut offset = usize::from(ppn) - usize::from(self.base_ppn);
        let mut descriptors = self.descriptors.lock();
        while descriptors[offset].has(FrameDescriptorFlags::TAKEN)
            && !descriptors[offset].has(FrameDescriptorFlags::LAST)
        {
            descriptors[offset].clear();
            offset += 1;
        }
        assert!(
            descriptors[offset].has(FrameDescriptorFlags::LAST),
            "possible double-free detected! (not taken found before last)"
        );
        descriptors[offset].clear();
    }

    /// Find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(descriptors: &[FrameDescriptor], pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let mut current_pages_begin = None;
        for (pages_end, descriptor) in descriptors.iter().enumerate() {
            if descriptor.has(FrameDescriptorFlags::TAKEN) {
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

bitflags! {
    #[derive(Clone, Copy)]
    struct FrameDescriptorFlags: u8 {
        const TAKEN = 1 << 0;
        const LAST = 1 << 1;
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct FrameDescriptor(u8);

impl FrameDescriptor {
    /// Enable the bit corresponding to the given page type.
    fn set(&mut self, flags: FrameDescriptorFlags) {
        self.0 |= flags.bits();
    }

    /// Return true of the given flag is set.
    fn has(&self, flags: FrameDescriptorFlags) -> bool {
        self.0 & flags.bits() == flags.bits()
    }

    /// Clear all previously set flags.
    fn clear(&mut self) {
        self.0 = 0;
    }
}
