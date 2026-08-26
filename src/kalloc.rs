//! Physical memory allocator that allocates whole 4096-byte frames.
//!
//! This module implements a buddy allocator that manages a single contiguous physical memory region
//! and splits it into smaller power-of-two sized blocks of frames. The allocator always fulfills
//! allocation requests with blocks whose sizes are the smallest power of two that is greater than
//! or equal to the requested size.
//!
//! The maximum allocation order is defined by `MAX_ORDER`, which is 12. As a result, the allocator
//! can allocate up to 4096 contiguous frames at once.

use core::{fmt, slice};

use crate::{
    HEAP_ADDR, MEM_ADDR, MEM_SIZE, PAGE_SIZE,
    mem::{paddr::PhysAddr, ppn::PhysPageNumber},
};

const MAX_ORDER: usize = 12;

/// A global physical memory allocator that allocates memory from the region between [`HEAP_ADDR`]
/// and the end of the physical memory.
pub struct Kmem {
    alloc: spin::Mutex<BuddyAlloc>,
}

impl Kmem {
    /// Initialize the global physical memory allocator.
    pub fn init() {
        let kmem = Self::get();
        unsafe {
            kmem.alloc
                .lock()
                .init(PhysAddr::new(HEAP_ADDR), (MEM_ADDR + MEM_SIZE) - HEAP_ADDR);
        }
    }

    /// Returns a reference to the global physical memory allocator.
    pub fn get() -> &'static Self {
        static KMEM: Kmem = Kmem {
            alloc: spin::Mutex::new(BuddyAlloc::empty()),
        };
        &KMEM
    }

    /// Gets exclusive access to the allocator and allocates a contiguous block of physical memory
    pub fn alloc(&self, num_frames: usize) -> Option<PhysPageNumber> {
        self.alloc.lock().alloc(num_frames)
    }

    /// Gets exclusive access to the allocator and deallocates a previously allocated block of physical memory.
    pub fn dealloc(&self, ppn: PhysPageNumber) {
        self.alloc.lock().dealloc(ppn);
    }
}

/// A buddy physical memory allocator.
///
/// [`BuddyAlloc`] manages physical memory by dividing it into blocks of sizes that are powers of two.
/// Each block's state is maintained in an array of [`Header`].
struct BuddyAlloc {
    /// The starting physical page number of the memory region managed by this allocator.
    addr: PhysPageNumber,
    /// An array of metadata headers for each frame in the managed region.
    headers: &'static mut [Header],
    /// Free lists for each allocation order.
    /// `free_list[i]` contains the head index of the free list for blocks of size 2^i frames.
    free_list: [Option<usize>; MAX_ORDER + 1],
}

impl fmt::Debug for BuddyAlloc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "frames: {:p} -> {:p} ({} frames, {} KB)",
            self.addr,
            self.addr + self.headers.len(),
            self.headers.len(),
            PAGE_SIZE * self.headers.len() / 1024,
        )?;
        writeln!(f, "free: {{")?;
        let mut total_free_frames = 0;
        for order in (0..=MAX_ORDER).rev() {
            let mut free_blocks = 0;
            let mut next = self.free_list[order];
            while let Some(curr) = next {
                free_blocks += 1;
                next = self.headers[curr].next;
            }
            let free_frames = free_blocks * (1 << order);
            total_free_frames += free_frames;
            writeln!(
                f,
                "  [{order:>02}] {free_blocks} blocks, {free_frames} frames, {} KB",
                PAGE_SIZE * free_frames / 1024
            )?;
        }
        writeln!(
            f,
            "}} {} frames, {} KB",
            total_free_frames,
            PAGE_SIZE * total_free_frames / 1024
        )
    }
}

impl BuddyAlloc {
    /// Creates a new, empty buddy allocator with no memory.
    const fn empty() -> Self {
        Self {
            addr: PhysPageNumber::new(0),
            headers: &mut [],
            free_list: [const { None }; MAX_ORDER + 1],
        }
    }

    /// Initializes the buddy allocator with a region of physical memory.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that the specified memory region `[addr, addr + len)` is valid,
    /// unused physical memory, and will not be accessed by other parts of the system without
    /// going through this allocator.
    unsafe fn init(&mut self, addr: PhysAddr, len: usize) {
        let header_start_addr = addr.align_up(align_of::<Header>());
        let alloc_end_ppn = addr.wrapping_add(len).page_number();
        let useable_len = alloc_end_ppn
            .addr()
            .offset_from(header_start_addr)
            .unwrap_or(0);
        let mut unprovisioned_frames = useable_len / (size_of::<Header>() + PAGE_SIZE);
        self.headers = Header::slice_from_ppn_mut(header_start_addr, unprovisioned_frames);
        self.addr = header_start_addr
            .wrapping_add(size_of_val(self.headers))
            .align_up(PAGE_SIZE)
            .page_number();
        let mut frame_idx = 0;
        for order in (0..=MAX_ORDER).rev() {
            let nth_order_frames = 1 << order;
            while unprovisioned_frames >= nth_order_frames {
                self.push_free(order, frame_idx);
                frame_idx += nth_order_frames;
                unprovisioned_frames -= nth_order_frames;
            }
        }
    }

    /// Allocates a contiguous block of physical memory.
    ///
    /// The allocator will round up `num_frames` to the nearest power of two and allocate a block of that size.
    ///
    /// Returns the starting physical page number of the allocated block, or [`None`] if the allocation fails
    /// (e.g., if there is not enough contiguous free memory).
    fn alloc(&mut self, num_frames: usize) -> Option<PhysPageNumber> {
        let order = num_frames.next_power_of_two().highest_one()? as usize;
        for o in order..=MAX_ORDER {
            let Some(idx) = self.pop_free(o) else {
                continue;
            };
            for o in (order..o).rev() {
                let buddy_idx = idx ^ (1 << o);
                self.push_free(o, buddy_idx);
            }
            return Some(self.addr + idx);
        }
        None
    }

    /// Deallocates a previously allocated block of physical memory.
    ///
    /// The `ppn` must be the starting physical page number of a block previously returned by [`Self::alloc`].
    /// The allocator determines the size of the block from its metadata header.
    ///
    /// # Panics
    ///
    /// Panics if the `ppn` is out of bounds of the memory region managed by this allocator.
    fn dealloc(&mut self, ppn: PhysPageNumber) {
        assert!(ppn >= self.addr, "page number should be bounded");
        let mut idx = ppn - self.addr;
        let mut order = self.headers[idx].order as usize;
        while order < MAX_ORDER {
            let buddy_idx = idx ^ (1 << order);
            if buddy_idx >= self.headers.len() {
                break;
            }
            if self.headers[buddy_idx].taken || self.headers[buddy_idx].order as usize != order {
                break;
            }
            self.remove_free(order, buddy_idx);
            idx = idx.min(buddy_idx);
            order += 1;
        }
        self.push_free(order, idx);
    }

    /// Removes and returns the first free block from the free list of the specified order.
    #[expect(clippy::cast_possible_truncation)]
    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let next = &mut self.free_list[order];
        next.take().inspect(|&idx| {
            *next = self.headers[idx].next.take();
            self.headers[idx].order = order as u8;
            self.headers[idx].taken = true;
        })
    }

    /// Adds a block to the front of the free list of the specified order.
    #[expect(clippy::cast_possible_truncation)]
    const fn push_free(&mut self, order: usize, idx: usize) {
        let next = &mut self.free_list[order];
        self.headers[idx].next = next.replace(idx);
        self.headers[idx].order = order as u8;
        self.headers[idx].taken = false;
    }

    /// Removes a specific block from the free list of the specified order.
    const fn remove_free(&mut self, order: usize, idx: usize) {
        let mut prev: Option<usize> = None;
        let mut next = self.free_list[order];
        while let Some(curr_idx) = next {
            if curr_idx == idx {
                if let Some(prev_idx) = prev {
                    self.headers[prev_idx].next = self.headers[curr_idx].next.take();
                } else {
                    self.free_list[order] = self.headers[curr_idx].next.take();
                }
                break;
            }
            prev = Some(curr_idx);
            next = self.headers[curr_idx].next;
        }
    }
}

/// Metadata header for a physical memory block.
///
/// Each frame in the managed region has a corresponding [`Header`] that stores
/// the linked list pointer for the free list, the order of the block it belongs to,
/// and whether the block is currently allocated.
#[derive(Default, Debug)]
struct Header {
    /// The index of the next block in the free list for this block's order.
    next: Option<usize>,
    /// The order of the block (size = 2^order frames).
    order: u8,
    /// Indicates whether the block is currently allocated.
    taken: bool,
}

impl Header {
    /// Creates a mutable slice of [`Header`] starting at the given physical address.
    ///
    /// # Safety
    ///
    /// The memory region starting at `addr` with size `len * size_of::<Header>()` must be valid,
    /// exclusively accessible, and correctly aligned. The memory is initialized with default values.
    fn slice_from_ppn_mut(addr: PhysAddr, len: usize) -> &'static mut [Self] {
        let headers_ptr = unsafe { addr.direct().as_ptr_mut::<Self>() };
        for i in 0..len {
            unsafe {
                headers_ptr.add(i).write(Self::default());
            }
        }
        unsafe { slice::from_raw_parts_mut(headers_ptr, len) }
    }
}
