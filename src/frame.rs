//! Functions and types for managing physical frames.

use core::{fmt, mem::MaybeUninit, mem::size_of, num::NonZero, slice};

use spin::Once;

use crate::{HEAP_SIZE, HEAP_START, align_value, sv39::PhysAddr};

/// Page size as an exponent of 2.
pub const FRAME_ORDER: usize = 12;

/// Page size in bytes.
pub const FRAME_SIZE: usize = 1 << FRAME_ORDER;

static ALLOCATOR: Once<Allocator> = Once::new();

/// Initialize the memory management system.
pub fn initialize() {
    ALLOCATOR.call_once(|| {
        let heap_start = unsafe { NonZero::new(HEAP_START) };
        let heap_size = unsafe { NonZero::new(HEAP_SIZE) };
        Allocator::new(
            heap_start.expect("non-zero heap start"),
            heap_size.expect("non-zero heap size"),
        )
    });
}

/// Grabs the physical frame allocator.
pub fn allocator() -> &'static Allocator {
    ALLOCATOR.get().expect("frame allocator initialized")
}

/// An allocator for 4096-byte physical frames.
#[derive(Debug)]
pub struct Allocator {
    descriptors: spin::Mutex<&'static mut [Descriptor]>,
    alloc_start_addr: PhysAddr,
}

impl fmt::Display for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();

        let begin = PhysAddr::from(descriptors.as_ptr());
        let end = begin + size_of::<Descriptor>() * descriptors.len();

        let alloc_total_size = descriptors.len() * FRAME_SIZE;
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
            let is_taken = descriptor.contains(DescriptorFlag::Taken);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.contains(DescriptorFlag::Last);
            if is_last {
                current_pages_begin.take();
                let addr_begin = self.alloc_start_addr + pages_begin * FRAME_SIZE;
                let addr_end = self.alloc_start_addr + page_end * FRAME_SIZE;
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
            count_taken * FRAME_SIZE
        )?;
        writeln!(
            f,
            "free: {:>6} pages ({:>10} bytes).",
            count_free,
            count_free * FRAME_SIZE
        )?;
        writeln!(f, "------------------------------------")?;
        Ok(())
    }
}

impl Allocator {
    /// Creates a new frame allocator given the heap's start address and size.
    pub fn new(heap_start: NonZero<usize>, heap_size: NonZero<usize>) -> Self {
        let desc_size = size_of::<Descriptor>();
        let pages = heap_size.get() / (FRAME_SIZE + desc_size);

        let alloc_start_addr = PhysAddr::from(align_value(
            heap_start.get() + pages * desc_size,
            FRAME_ORDER,
        ));

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start.get() as *mut Descriptor, pages) };

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
            (offset..offset + pages).for_each(|i| descriptors[i].set(DescriptorFlag::Taken));
            descriptors[offset + pages - 1].set(DescriptorFlag::Last);
            self.alloc_start_addr + FRAME_SIZE * offset
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
                    (FRAME_SIZE * pages) / 8,
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
        let mut page = page_offset / FRAME_SIZE;
        let mut descriptors = self.descriptors.lock();
        while descriptors[page].contains(DescriptorFlag::Taken)
            && !descriptors[page].contains(DescriptorFlag::Last)
        {
            descriptors[page].clear();
            page += 1;
        }
        assert!(
            descriptors[page].contains(DescriptorFlag::Last),
            "Possible double-free detected! (Not taken found before last)"
        );
        descriptors[page].clear();
    }

    /// find a first address of a contiguous region of one or more free pages.
    fn find_free_pages(descriptors: &[Descriptor], pages: usize) -> Option<usize> {
        assert!(pages > 0);
        let mut current_pages_begin = None;
        for (pages_end, descriptor) in descriptors.iter().enumerate() {
            if descriptor.contains(DescriptorFlag::Taken) {
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
enum DescriptorFlag {
    /// page has been taken by the allocator.
    Taken = 1 << 0,

    /// page is the last one in the allocated pages.
    Last = 1 << 1,
}

#[derive(Debug)]
struct Descriptor(u8);

impl Descriptor {
    /// enable the bit corresponding to the given page type.
    fn set(&mut self, flag: DescriptorFlag) {
        self.0 |= flag as u8;
    }

    /// return true of the given flag is set.
    fn contains(&self, flag: DescriptorFlag) -> bool {
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
