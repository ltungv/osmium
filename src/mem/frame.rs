//! Functions and types for managing physical frames.

use core::fmt;

use bitflags::bitflags;

use crate::{PAGE_SIZE, addr::PhysAddr, mem::ppn::PhysPageNumber};

/// An allocator for 4096-byte physical frames.
pub struct Allocator {
    base_ppn: PhysPageNumber,
    descriptors: &'static mut [FrameDescriptor],
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let desc_total_size = core::mem::size_of_val(self.descriptors);
        let desc_begin = PhysAddr::from(self.descriptors.as_ptr() as usize);
        let desc_end = desc_begin + desc_total_size;

        let alloc_begin = self.base_ppn;
        let alloc_end = alloc_begin + PAGE_SIZE;
        let alloc_total_size = self.descriptors.len() * PAGE_SIZE;

        writeln!(f, "------------------------------------")?;
        writeln!(
            f,
            "PageAllocator [pages={} size={}]",
            self.descriptors.len(),
            alloc_total_size,
        )?;
        writeln!(f, "desc: {desc_begin:?} -> {desc_end:?}")?;
        writeln!(f, "phys: {alloc_begin:?} -> {alloc_end:?}")?;
        writeln!(f, "------------------------------------")?;
        let mut current_pages_begin = None;
        let mut count_taken = 0;
        for (page_end, descriptor) in self.descriptors.iter().enumerate() {
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
        let count_free = self.descriptors.len() - count_taken;
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
    // /// Creates a new frame allocator given the physical start address and size.
    // ///
    // /// # Safety
    // ///
    // /// Caller must guarantee that the memory region from `base` to `base + size`
    // /// is physically available for this allocator to manage.
    // pub(crate) unsafe fn new(base: PhysAddr, size: usize) -> Self {
    //     let capacity = size / (size_of::<FrameDescriptor>() + PAGE_SIZE);
    //     let descriptors = unsafe {
    //         slice::from_raw_parts_mut(base.as_ptr_mut().cast::<FrameDescriptor>(), capacity)
    //     };
    //     for descriptor in descriptors.iter_mut() {
    //         descriptor.clear();
    //     }
    //     let base_ppn = (base + size_of::<FrameDescriptor>() * descriptors.len()).ceil();
    //     Self {
    //         base_ppn,
    //         descriptors,
    //     }
    // }

    /// Allocates a contiguous region of `pages` and returns the address at the start of the region.
    /// If there's not enough memory, returns `None`.
    pub fn alloc(&mut self, pages: usize) -> Option<PhysPageNumber> {
        let offset = Self::find_free_pages(self.descriptors, pages)?;

        self.descriptors[offset + pages - 1].set(FrameDescriptorFlags::LAST);
        self.descriptors[offset..offset + pages]
            .iter_mut()
            .for_each(|d| d.set(FrameDescriptorFlags::TAKEN));

        Some(self.base_ppn + offset)
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub(crate) fn zalloc(&mut self, pages: usize) -> Option<PhysPageNumber> {
        self.alloc(pages)
            .inspect(|frame| frame.as_slice_mut::<u8>().fill(0))
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure this function is only called with the starting address of a contiguous
    /// page region that was previously allocated by this frame allocator.
    pub(crate) unsafe fn dealloc(&mut self, ppn: PhysPageNumber) {
        assert!(ppn >= self.base_ppn);
        let mut offset = usize::from(ppn) - usize::from(self.base_ppn);
        while self.descriptors[offset].has(FrameDescriptorFlags::TAKEN)
            && !self.descriptors[offset].has(FrameDescriptorFlags::LAST)
        {
            self.descriptors[offset].clear();
            offset += 1;
        }
        assert!(
            self.descriptors[offset].has(FrameDescriptorFlags::LAST),
            "possible double-free detected! (not taken found before last)"
        );
        self.descriptors[offset].clear();
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
    /// Clear all previously set flags.
    fn clear(&mut self) {
        self.0 = 0;
    }

    /// Return true of the given flag is set.
    fn has(&self, flags: FrameDescriptorFlags) -> bool {
        self.0 & flags.bits() == flags.bits()
    }

    /// Enable the bit corresponding to the given page type.
    fn set(&mut self, flags: FrameDescriptorFlags) {
        self.0 |= flags.bits();
    }
}
