use core::{fmt, slice};

use crate::{PAGE_SIZE, addr::PhysAddr, mem::ppn::PhysPageNumber, print, println};

const MAX_ORDER: usize = 12;

pub struct BuddyAllocator {
    base: PhysPageNumber,
    free: [FreeList; MAX_ORDER + 1],
    headers: &'static mut [FrameHeader],
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
            self.base,
            self.base + self.headers.len(),
            self.headers.len(),
            PAGE_SIZE * self.headers.len() / 1024,
        )?;
        writeln!(f, "free: {{")?;
        let mut free_frames = 0;
        for order in (0..=MAX_ORDER).rev() {
            let mut order_free_segments = 0;
            let mut next = self.free[order].head;
            while let Some(curr) = next {
                order_free_segments += 1;
                next = self.headers[curr].next;
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
    pub(crate) unsafe fn new(mem_base: PhysAddr, mem_size: usize) -> Self {
        assert!(mem_size > PAGE_SIZE,);
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

        let mut allocator = Self {
            base: (aligned_base + size_of::<FrameHeader>() * num_frames).ceil(),
            free: [const { FreeList::new() }; MAX_ORDER + 1],
            headers: unsafe { slice::from_raw_parts_mut(headers_base_ptr, num_frames) },
        };

        let mut frame_idx = 0;
        let mut unprovision_frames = allocator.headers.len();
        for order in (0..=MAX_ORDER).rev() {
            let nth_order_frames = 1 << order;
            while unprovision_frames >= nth_order_frames {
                assert!(!allocator.headers[frame_idx].taken);
                allocator.headers[frame_idx].order = order as u8;
                allocator.push_free(order, frame_idx);
                frame_idx += nth_order_frames;
                unprovision_frames -= nth_order_frames;
            }
        }

        allocator
    }

    pub(crate) fn alloc(&mut self, order: usize) -> Option<PhysPageNumber> {
        // Search for the smallest order that is free to make space for the allocation.
        for o in order..=MAX_ORDER {
            let Some(taken_idx) = self.pop_free(o) else {
                continue;
            };
            // At this point, we found a free region that can accomodate the allocation request.
            // However, the region can be larger than the size given by the requested order. We then
            // iterate backwards from where we found a free region until reaching the requested
            // order. At each order, we take a free region, split it in half, and take one of the
            // halves while returning the other to the free list.
            for o in (order..o).rev() {
                let buddy_idx = taken_idx ^ (1 << o);
                self.headers[buddy_idx].order = o as u8;
                self.push_free(o, buddy_idx);
                println!("buddy {:?} at order={o}", self.base + buddy_idx);
            }
            self.headers[taken_idx].order = order as u8;
            self.headers[taken_idx].taken = true;
            return Some(self.base + taken_idx);
        }
        None
    }

    pub(crate) fn zalloc(&mut self, order: usize) -> Option<PhysPageNumber> {
        self.alloc(order).inspect(|&base| {
            for i in 0..1 << order {
                let ppn = base + i;
                ppn.as_slice_mut().fill(0);
            }
        })
    }

    pub(crate) fn dealloc(&mut self, ppn: PhysPageNumber) {
        assert!(ppn >= self.base);
        let mut free_idx = usize::from(ppn) - usize::from(self.base);
        let mut order = self.headers[free_idx].order as usize;
        while order < MAX_ORDER {
            let buddy_idx = free_idx ^ (1 << order);
            if buddy_idx >= self.headers.len() {
                break;
            }
            if self.headers[buddy_idx].taken || self.headers[buddy_idx].order as usize != order {
                break;
            }
            println!(
                "merge {ppn:?} and {:?} at order={order}",
                self.base + buddy_idx
            );
            self.remove_free(order, buddy_idx);
            free_idx = free_idx.min(buddy_idx);
            order += 1;
        }
        self.headers[free_idx].order = order as u8;
        self.headers[free_idx].taken = false;
        self.push_free(order, free_idx);
    }

    fn remove_free(&mut self, order: usize, idx: usize) {
        let mut prev: Option<usize> = None;
        let mut next = self.free[order].head;
        while let Some(curr_idx) = next {
            if curr_idx == idx {
                if let Some(prev_idx) = prev {
                    self.headers[prev_idx].next = self.headers[curr_idx].next.take();
                } else {
                    self.free[order].head = self.headers[curr_idx].next.take();
                }
                break;
            }
            prev = Some(curr_idx);
            next = self.headers[curr_idx].next;
        }
    }

    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let segment = &mut self.free[order];
        segment.head.take().inspect(|&head| {
            segment.head = self.headers[head].next.take();
        })
    }

    fn push_free(&mut self, order: usize, idx: usize) {
        let segment = &mut self.free[order];
        self.headers[idx].next = segment.head.replace(idx);
    }
}

#[derive(Default)]
struct FrameHeader {
    next: Option<usize>,
    order: u8,
    taken: bool,
}

struct FreeList {
    head: Option<usize>,
}

impl FreeList {
    const fn new() -> Self {
        Self { head: None }
    }
}
