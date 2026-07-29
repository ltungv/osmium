//! Functions and types for managing physical frames.

use core::{fmt, mem::size_of, ops, slice};

use bitflags::bitflags;

use crate::{
    align_value,
    mem::{PAGE_SIZE, PAGE_SIZE_BITS},
};

/// An allocator for 4096-byte physical frames.
pub(crate) struct Allocator {
    descriptors: spin::Mutex<&'static mut [FrameDescriptor]>,
    alloc_start: FrameNumber,
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();
        let desc_begin = descriptors.as_ptr() as usize;
        let desc_end = desc_begin + size_of::<FrameDescriptor>() * descriptors.len();

        let alloc_total_size = descriptors.len() * PAGE_SIZE;
        let alloc_begin = self.alloc_start;
        let alloc_end = alloc_begin + descriptors.len();

        writeln!(f, "------------------------------------")?;
        writeln!(
            f,
            "PageAllocator [pages={} size={}]",
            descriptors.len(),
            alloc_total_size,
        )?;
        writeln!(f, "desc: 0x{:x} -> 0x{:x}", desc_begin, desc_end)?;
        writeln!(
            f,
            "phys: 0x{:x} -> 0x{:x}",
            alloc_begin.addr(),
            alloc_end.addr()
        )?;
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
                let alloc_begin = self.alloc_start + pages_begin;
                let alloc_end = self.alloc_start + page_end;
                writeln!(
                    f,
                    "[{:>4}] 0x{:x} -> 0x{:x}: {:>3} page(s)",
                    pages_begin,
                    alloc_begin.addr(),
                    alloc_end.addr(),
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
        let alloc_start = FrameNumber::try_from(align_value(
            heap_start + size_of::<FrameDescriptor>() * pages,
            PAGE_SIZE_BITS,
        ))
        .expect("start of allocation address is aligned");

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start as *mut FrameDescriptor, pages) };

        for descriptor in descriptors.iter_mut() {
            descriptor.clear();
        }

        Self {
            alloc_start,
            descriptors: spin::Mutex::new(descriptors),
        }
    }

    /// Allocates a contiguous region of `pages` and returns the address at the start of the region.
    /// If there's not enough memory, returns `None`.
    pub(crate) fn alloc(&self, pages: usize) -> Option<FrameNumber> {
        let mut descriptors = self.descriptors.lock();
        let offset = Self::find_free_pages(&descriptors, pages)?;
        descriptors[offset + pages - 1].set(FrameDescriptorFlags::LAST);
        for i in offset..offset + pages {
            descriptors[i].set(FrameDescriptorFlags::TAKEN);
        }
        Some(self.alloc_start + offset)
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub(crate) fn zalloc(&self, pages: usize) -> Option<FrameNumber> {
        self.alloc(pages).inspect(|id| unsafe {
            core::ptr::write_bytes(id.addr() as *mut u8, 0, PAGE_SIZE * pages);
        })
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure this function is only called with the starting address of a contiguous
    /// page region that was previously allocated by this frame allocator.
    pub(crate) unsafe fn dealloc(&self, id: FrameNumber) {
        assert!(id >= self.alloc_start);
        let mut offset = id.0 - self.alloc_start.0;
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

#[derive(Debug)]
pub(crate) struct UnalignedFrameAddressError;

impl fmt::Display for UnalignedFrameAddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frame address must be aligned to {PAGE_SIZE}")
    }
}

/// A frame's start address shifted right by `PAGE_SIZE_BITS`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FrameNumber(usize);

impl ops::Add<usize> for FrameNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl TryFrom<usize> for FrameNumber {
    type Error = UnalignedFrameAddressError;

    fn try_from(addr: usize) -> Result<Self, Self::Error> {
        let mask = (1 << PAGE_SIZE_BITS) - 1;
        if addr & mask != 0 {
            return Err(UnalignedFrameAddressError);
        }
        Ok(Self(addr >> PAGE_SIZE_BITS))
    }
}

impl FrameNumber {
    // /// Returns the address to the start of frame.
    pub(crate) fn addr(self) -> usize {
        self.0 << PAGE_SIZE_BITS
    }
}
