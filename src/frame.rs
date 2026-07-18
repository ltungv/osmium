//! Functions and types for managing physical frames.

use core::{
    fmt::{self, Display},
    mem::{MaybeUninit, size_of},
    num::NonZero,
    ops, slice,
};

use spin::Once;

use crate::{HEAP_SIZE, HEAP_START, align_value};

/// Frame size as an exponent of 2.
pub const FRAME_ORDER: usize = 12;

/// Frame size in bytes.
pub const FRAME_SIZE: usize = 1 << FRAME_ORDER;

static ALLOCATOR: Once<FrameAllocator> = Once::new();

/// Initialize the memory management system.
pub fn initialize() {
    ALLOCATOR.call_once(|| {
        let heap_start = unsafe { NonZero::new(HEAP_START) };
        let heap_size = unsafe { NonZero::new(HEAP_SIZE) };
        FrameAllocator::new(
            heap_start.expect("non-zero heap start"),
            heap_size.expect("non-zero heap size"),
        )
    });
}

/// Grabs the physical frame allocator.
pub fn frame_allocator() -> &'static FrameAllocator {
    ALLOCATOR.get().expect("initialized frame allocator")
}

/// Errors from interacting with physical frames.
#[derive(Debug)]
pub enum FrameError {
    /// Frame address is not aligned to `FRAME_SIZE`.
    UnalignedAddress(usize),
}

impl Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::UnalignedAddress(addr) => {
                write!(f, "0x{addr:x} is not aligned to {FRAME_SIZE}")
            }
        }
    }
}

/// A frame's start address shifted right by `FRAME_ORDER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameId(usize);

impl ops::Add<usize> for FrameId {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for FrameId {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl TryFrom<usize> for FrameId {
    type Error = FrameError;

    fn try_from(addr: usize) -> Result<Self, Self::Error> {
        let mask = (1 << FRAME_ORDER) - 1;
        if addr & mask != 0 {
            return Err(FrameError::UnalignedAddress(addr));
        }
        Ok(Self(addr >> FRAME_ORDER))
    }
}

impl FrameId {
    /// Returns the address to the frame.
    pub fn addr(&self) -> usize {
        self.0 << FRAME_ORDER
    }
}

/// An allocator for 4096-byte physical frames.
pub struct FrameAllocator {
    descriptors: spin::Mutex<&'static mut [FrameDescriptor]>,
    alloc_start: FrameId,
}

impl fmt::Debug for FrameAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descriptors = self.descriptors.lock();

        let begin = descriptors.as_ptr() as usize;
        let end = begin + size_of::<FrameDescriptor>() * descriptors.len();

        let alloc_total_size = descriptors.len() * FRAME_SIZE;
        let alloc_begin = self.alloc_start;
        let alloc_end = alloc_begin + alloc_total_size;

        writeln!(f, "------------------------------------")?;
        writeln!(
            f,
            "PageAllocator [pages={} size={}]",
            descriptors.len(),
            alloc_total_size,
        )?;
        writeln!(f, "desc: 0x{:x} -> 0x{:x}", begin, end)?;
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
            let is_taken = descriptor.contains(FrameDescriptorFlag::Taken);
            if !is_taken {
                continue;
            }
            count_taken += 1;
            let pages_begin = *current_pages_begin.get_or_insert(page_end);
            let is_last = descriptor.contains(FrameDescriptorFlag::Last);
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
            count_taken * FRAME_SIZE
        )?;
        writeln!(
            f,
            "free: {:>6} pages ({:>10} bytes).",
            count_free,
            count_free * FRAME_SIZE
        )?;
        write!(f, "------------------------------------")?;
        Ok(())
    }
}

impl FrameAllocator {
    /// Creates a new frame allocator given the heap's start address and size.
    pub fn new(heap_start: NonZero<usize>, heap_size: NonZero<usize>) -> Self {
        let pages = heap_size.get() / (size_of::<FrameDescriptor>() + FRAME_SIZE);
        let alloc_start = FrameId::try_from(align_value(
            heap_start.get() + size_of::<FrameDescriptor>() * pages,
            FRAME_ORDER,
        ))
        .expect("allocation start address is aligned");

        let descriptors =
            unsafe { slice::from_raw_parts_mut(heap_start.get() as *mut FrameDescriptor, pages) };

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
    pub fn alloc(&self, pages: usize) -> Option<FrameId> {
        assert!(pages > 0);
        let mut descriptors = self.descriptors.lock();
        Self::find_free_pages(&descriptors, pages).map(|offset| {
            descriptors[offset + pages - 1].set(FrameDescriptorFlag::Last);
            for i in offset..offset + pages {
                descriptors[i].set(FrameDescriptorFlag::Taken);
            }
            self.alloc_start + offset
        })
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub fn zalloc(&self, pages: usize) -> Option<FrameId> {
        self.alloc(pages).inspect(|frame_id| {
            let qwords = unsafe {
                slice::from_raw_parts_mut(
                    frame_id.addr() as *mut MaybeUninit<u64>,
                    (FRAME_SIZE * pages) / size_of::<u64>(),
                )
            };
            for qword in qwords {
                qword.write(0);
            }
        })
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure that this function is only called with the starting address of a
    /// continguous page region
    pub unsafe fn dealloc(&self, id: FrameId) {
        assert!(id > self.alloc_start);
        let mut offset = id - self.alloc_start;
        let mut descriptors = self.descriptors.lock();
        while descriptors[offset].contains(FrameDescriptorFlag::Taken)
            && !descriptors[offset].contains(FrameDescriptorFlag::Last)
        {
            descriptors[offset].clear();
            offset += 1;
        }
        assert!(
            descriptors[offset].contains(FrameDescriptorFlag::Last),
            "possible double-free detected! (not taken found before last)"
        );
        descriptors[offset].clear();
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
    /// Page has been taken by the allocator.
    Taken = 1 << 0,

    /// Page is the last one in the allocated pages.
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
