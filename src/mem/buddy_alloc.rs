use core::{
    fmt::{self, write},
    ops::{Index, IndexMut},
    slice,
};

use crate::{PAGE_SIZE, addr::PhysAddr, mem::ppn::PhysPageNumber, print, println};

const MAX_ORDER: usize = 12;

const MAX_PAGES: usize = 1 << MAX_ORDER;

pub struct BuddyAllocator {
    frames: Frames,
    free: [FreeList; MAX_ORDER + 1],
}

// SAFETY: Constructing a `BuddyAllocator` requires the caller to uphold the invariants that the
// memory region, as designated by the base pointer and the size, is given exclusively to the
// allocator and has a `'static` lifetime. We want `BuddyAllocator` to be `Send` so we can wrap it
// in a `spin::Mutex`, and enable concurrent share access to the allocator.
unsafe impl Send for BuddyAllocator {}

impl fmt::Debug for BuddyAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "BuddyAllocator")?;
        writeln!(
            f,
            "frames: {:?} -> {:?} ({} frames, {} KB)",
            self.frames.base,
            self.frames.base + self.frames.headers.len(),
            self.frames.headers.len(),
            PAGE_SIZE * self.frames.headers.len() / 1024,
        )?;
        writeln!(f, "free: {{")?;
        let mut free_frames = 0;
        for order in (0..=MAX_ORDER).rev() {
            let mut order_free_segments = 0;
            let mut next = self.free[order].head;
            while let Some(curr) = next {
                order_free_segments += 1;
                next = self.frames[curr].next;
            }
            let order_free_frames = order_free_segments * (1 << order);
            free_frames += order_free_frames;
            writeln!(
                f,
                "  [{order:>02}] {order_free_segments} segments, {order_free_frames} frames, {} KB",
                PAGE_SIZE * order_free_frames / 1024
            )?;
        }
        write!(
            f,
            "}} {} frames, {} KB",
            free_frames,
            PAGE_SIZE * free_frames / 1024
        )
    }
}

#[allow(clippy::cast_possible_truncation)]
impl BuddyAllocator {
    pub unsafe fn new(mem_base: PhysAddr, mem_size: usize) -> Self {
        assert!(mem_size > PAGE_SIZE,);
        let frames = unsafe { Frames::new(mem_base, mem_size) };
        let free = [const { FreeList::new() }; MAX_ORDER + 1];

        let mut allocator = Self { frames, free };
        let mut frame_ppn = allocator.frames.base;
        let mut unprovision_frames = allocator.frames.headers.len();

        for order in (0..=MAX_ORDER).rev() {
            let nth_order_frames = 1 << order;
            while unprovision_frames >= nth_order_frames {
                assert!(!allocator.frames[frame_ppn].taken);
                allocator.frames[frame_ppn].order = order as u8;
                allocator.push_free(order, frame_ppn);
                frame_ppn = frame_ppn + nth_order_frames;
                unprovision_frames -= nth_order_frames;
            }
        }

        allocator
    }

    pub fn alloc(&mut self, order: usize) -> Option<PhysPageNumber> {
        // Search for the smallest order that is free to make space for the allocation.
        for o in order..=MAX_ORDER {
            let Some(free_ppn) = self.pop_free(o) else {
                continue;
            };
            // At this point, we found a free region that can accomodate the allocation request.
            // However, the region can be larger than the size given by the requested order. We then
            // iterate backwards from where we found a free region until reaching the requested
            // order. At each order, we take a free region, split it in half, and take one of the
            // halves while returning the other to the free list.
            for o in (order..o).rev() {
                let buddy_ppn = free_ppn ^ (1 << o);
                println!("free buddy {buddy_ppn:?} at order={o}");
                self.push_free(o, buddy_ppn);
            }
            println!("alloc {free_ppn:?} at order={order}");
            self.frames[free_ppn].order = order as u8;
            self.frames[free_ppn].taken = true;
            return Some(free_ppn);
        }
        None
    }

    pub fn dealloc(&mut self, mut ppn: PhysPageNumber) {
        assert!(ppn >= self.frames.base);
        let mut order = self.frames[ppn].order as usize;
        while order < MAX_ORDER {
            let buddy_ppn = ppn ^ (1 << order);
            if buddy_ppn >= self.frames.base + self.frames.headers.len() {
                break;
            }
            if self.frames[buddy_ppn].taken || self.frames[buddy_ppn].order as usize != order {
                break;
            }
            println!("merge {ppn:?} and {buddy_ppn:?} at order={order}");
            self.remove_free(order, buddy_ppn);
            ppn = ppn.min(buddy_ppn);
            order += 1;
        }
        println!("free {ppn:?} at order={order}");
        self.frames[ppn].order = order as u8;
        self.frames[ppn].taken = false;
        self.push_free(order, ppn);
    }

    fn remove_free(&mut self, order: usize, ppn: PhysPageNumber) {
        let mut prev_ppn = None;
        let mut next_ppn = self.free[order].head;
        while let Some(curr_ppn) = next_ppn {
            if curr_ppn == ppn {
                if let Some(prev_ppn) = prev_ppn {
                    self.frames[prev_ppn].next = self.frames[curr_ppn].next.take();
                } else {
                    self.free[order].head = self.frames[curr_ppn].next.take();
                }
                break;
            }
            prev_ppn = Some(curr_ppn);
            next_ppn = self.frames[curr_ppn].next;
        }
    }

    fn pop_free(&mut self, order: usize) -> Option<PhysPageNumber> {
        let segment = &mut self.free[order];
        segment.head.take().inspect(|&head| {
            segment.head = self.frames[head].next.take();
        })
    }

    fn push_free(&mut self, order: usize, ppn: PhysPageNumber) {
        let segment = &mut self.free[order];
        self.frames[ppn].next = segment.head.replace(ppn);
    }
}

pub struct Frames {
    base: PhysPageNumber,
    headers: &'static mut [FrameHeader],
}

impl Index<PhysPageNumber> for Frames {
    type Output = FrameHeader;

    fn index(&self, ppn: PhysPageNumber) -> &Self::Output {
        assert!(ppn >= self.base);
        let idx = usize::from(ppn) - usize::from(self.base);
        assert!(idx < self.headers.len());
        unsafe { self.headers.get_unchecked(idx) }
    }
}

impl IndexMut<PhysPageNumber> for Frames {
    fn index_mut(&mut self, ppn: PhysPageNumber) -> &mut Self::Output {
        assert!(ppn >= self.base);
        let idx = usize::from(ppn) - usize::from(self.base);
        assert!(idx < self.headers.len());
        unsafe { self.headers.get_unchecked_mut(idx) }
    }
}

impl Frames {
    unsafe fn new(mem_base: PhysAddr, mem_size: usize) -> Self {
        // Align the start of the memory region to the alignment of `Frame`. An array of frames is
        // stored starting from this memory addresss to hold metadata about the frames.
        let aligned_base = mem_base.align(align_of::<FrameHeader>());

        // Initialize the array of frames by writing directly into the raw pointers. We avoid
        // creating references into the array to prevent accidental access to uninitialized data.
        let headers_base_ptr = unsafe { aligned_base.as_ptr_mut::<FrameHeader>() };
        let num_frames = mem_size / (size_of::<FrameHeader>() + PAGE_SIZE);
        for i in 0..num_frames {
            unsafe {
                headers_base_ptr.add(i).write(FrameHeader::default());
            }
        }

        let headers = unsafe { slice::from_raw_parts_mut(headers_base_ptr, num_frames) };
        Self {
            base: (aligned_base + size_of_val(headers)).ceil(),
            headers,
        }
    }
}

pub struct FrameHeader {
    next: Option<PhysPageNumber>,
    order: u8,
    taken: bool,
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self {
            next: None,
            order: 0,
            taken: false,
        }
    }
}

struct FreeList {
    head: Option<PhysPageNumber>,
}

impl FreeList {
    const fn new() -> Self {
        Self { head: None }
    }
}
