//! Functions and types for managing physical frames.

use core::{
    fmt::{self, Display},
    mem::{MaybeUninit, size_of},
    num::NonZero,
    ops, slice,
};

use bitflags::bitflags;
use spin::Once;

use crate::{HEAP_SIZE, HEAP_START, align_value};

/// Frame size as an exponent of 2.
pub const FRAME_ORDER: usize = 12;

/// Frame size in bytes.
pub const FRAME_SIZE: usize = 1 << FRAME_ORDER;

static ALLOCATOR: Once<FrameAllocator> = Once::new();

/// Initialize the global frame allocator.
pub fn initialize() {
    ALLOCATOR.call_once(|| unsafe {
        let heap_start = NonZero::new(HEAP_START);
        let heap_size = NonZero::new(HEAP_SIZE);
        FrameAllocator::new(
            heap_start.expect("non-zero heap start"),
            heap_size.expect("non-zero heap size"),
        )
    });
}

/// Grabs the global physical frame allocator.
pub fn frame_allocator() -> &'static FrameAllocator {
    ALLOCATOR.get().expect("initialized frame allocator")
}

/// Errors from interacting with physical frames.
#[derive(Debug)]
pub enum Error {
    /// Frame address is not aligned to `FRAME_SIZE`.
    UnalignedAddress(usize),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnalignedAddress(addr) => {
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
    type Error = Error;

    fn try_from(addr: usize) -> Result<Self, Self::Error> {
        let mask = (1 << FRAME_ORDER) - 1;
        if addr & mask != 0 {
            return Err(Error::UnalignedAddress(addr));
        }
        Ok(Self(addr >> FRAME_ORDER))
    }
}

impl From<FrameId> for usize {
    fn from(id: FrameId) -> Self {
        id.0
    }
}

impl FrameId {
    /// Returns the address to the start of frame.
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
        let desc_begin = descriptors.as_ptr() as usize;
        let desc_end = desc_begin + size_of::<FrameDescriptor>() * descriptors.len();

        let alloc_total_size = descriptors.len() * FRAME_SIZE;
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
    ///
    /// # Safety
    ///
    /// Caller must guarantee that the memory region from `heap_start` to `heap_start + heap_size`
    /// is physically available for this allocator to manage.
    pub unsafe fn new(heap_start: NonZero<usize>, heap_size: NonZero<usize>) -> Self {
        let pages = heap_size.get() / (size_of::<FrameDescriptor>() + FRAME_SIZE);
        let alloc_start = FrameId::try_from(align_value(
            heap_start.get() + size_of::<FrameDescriptor>() * pages,
            FRAME_ORDER,
        ))
        .expect("allocation start address is aligned");

        let descriptors = unsafe {
            slice::from_raw_parts_mut(heap_start.get() as *mut MaybeUninit<FrameDescriptor>, pages)
        };

        for descriptor in descriptors.iter_mut() {
            descriptor.write(FrameDescriptor(0));
        }

        let descriptors = unsafe {
            core::mem::transmute::<
                &mut [core::mem::MaybeUninit<FrameDescriptor>],
                &mut [FrameDescriptor],
            >(descriptors)
        };

        Self {
            alloc_start,
            descriptors: spin::Mutex::new(descriptors),
        }
    }

    /// Allocates a contiguous region of `pages` and returns the address at the start of the region.
    /// If there's not enough memory, returns `None`.
    pub fn alloc(&self, pages: usize) -> Option<FrameId> {
        let mut descriptors = self.descriptors.lock();
        Self::find_free_pages(&descriptors, pages).map(|offset| {
            descriptors[offset + pages - 1].set(FrameDescriptorFlags::LAST);
            for i in offset..offset + pages {
                descriptors[i].set(FrameDescriptorFlags::TAKEN);
            }
            self.alloc_start + offset
        })
    }

    /// Allocates a contiguous region of `pages`, initializes the region to 0, and returns the address
    /// at the start of the region. If there's not enough memory, returns `None`.
    pub fn zalloc(&self, pages: usize) -> Option<FrameId> {
        self.alloc(pages).inspect(|frame_id| unsafe {
            core::ptr::write_bytes(frame_id.addr() as *mut u8, 0, FRAME_SIZE * pages);
        })
    }

    /// Deallocate a contiguous region starting at `ptr`.
    ///
    /// # Safety
    ///
    /// Caller must make sure this function is only called with the starting address of a contiguous
    /// page region that was previously allocated by this frame allocator.
    pub unsafe fn dealloc(&self, id: FrameId) {
        assert!(id >= self.alloc_start);
        let mut offset = id - self.alloc_start;
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

    /// find a first address of a contiguous region of one or more free pages.
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

#[derive(Debug)]
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
