use core::{fmt, slice};

use crate::{PAGE_SIZE, addr::PhysAddr, mem::ppn::PhysPageNumber};

const MAX_ORDER: usize = 12;

pub struct BuddyAlloc {
    ppn: PhysPageNumber,
    state: spin::Mutex<State>,
}

impl fmt::Debug for BuddyAlloc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.lock();
        writeln!(f, "BuddyAllocator")?;
        writeln!(
            f,
            "frames: {:?} -> {:?} ({} frames, {} KB)",
            self.ppn,
            self.ppn + state.headers.len(),
            state.headers.len(),
            PAGE_SIZE * state.headers.len() / 1024,
        )?;
        writeln!(f, "free: {{")?;
        let mut total_free_frames = 0;
        for order in (0..=MAX_ORDER).rev() {
            let mut free_blocks = 0;
            let mut next = state.free[order].head;
            while let Some(curr) = next {
                free_blocks += 1;
                next = state.headers[curr].next;
            }
            let free_frames = free_blocks * (1 << order);
            total_free_frames += free_frames;
            writeln!(
                f,
                "  [{order:>02}] {free_blocks} blocks, {free_frames} frames, {} KB",
                PAGE_SIZE * free_frames / 1024
            )?;
        }
        write!(
            f,
            "}} {} frames, {} KB",
            total_free_frames,
            PAGE_SIZE * total_free_frames / 1024
        )
    }
}

impl BuddyAlloc {
    pub unsafe fn new(addr: PhysAddr, len: usize) -> Option<Self> {
        let aligned_addr = addr.align(align_of::<Header>());
        let state = unsafe { State::new(aligned_addr, addr + len) };
        state.map(|s| Self {
            ppn: (aligned_addr + core::mem::size_of_val(s.headers)).ceil(),
            state: spin::Mutex::new(s),
        })
    }

    pub fn alloc(&self, order: usize) -> Option<PhysPageNumber> {
        self.state.lock().alloc(order).map(|idx| self.ppn + idx)
    }

    pub fn zalloc(&self, order: usize) -> Option<PhysPageNumber> {
        self.alloc(order).inspect(|&ppn| {
            for i in 0..1 << order {
                let ppn = ppn + i;
                ppn.as_slice_mut().fill(0);
            }
        })
    }

    pub fn dealloc(&self, ppn: PhysPageNumber) {
        assert!(ppn >= self.ppn);
        let idx = ppn - self.ppn;
        self.state.lock().dealloc(idx);
    }
}

struct State {
    free: [FreeList; MAX_ORDER + 1],
    headers: &'static mut [Header],
}

impl State {
    #[allow(clippy::cast_possible_truncation)]
    unsafe fn new(start: PhysAddr, end: PhysAddr) -> Option<Self> {
        if end <= start {
            return None;
        }
        let len = end - start;
        let headers_ptr = unsafe { start.as_ptr_mut::<Header>() };
        let mut unprovisioned_frames = len / (size_of::<Header>() + PAGE_SIZE);
        for i in 0..unprovisioned_frames {
            unsafe {
                headers_ptr.add(i).write(Header::default());
            }
        }
        let mut allocator = Self {
            free: [const { FreeList::new() }; MAX_ORDER + 1],
            headers: unsafe { slice::from_raw_parts_mut(headers_ptr, unprovisioned_frames) },
        };
        let mut frame_idx = 0;
        for order in (0..=MAX_ORDER).rev() {
            let nth_order_frames = 1 << order;
            while unprovisioned_frames >= nth_order_frames {
                assert!(!allocator.headers[frame_idx].taken);
                allocator.headers[frame_idx].order = order as u8;
                allocator.push_free(order, frame_idx);
                frame_idx += nth_order_frames;
                unprovisioned_frames -= nth_order_frames;
            }
        }
        Some(allocator)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn alloc(&mut self, order: usize) -> Option<usize> {
        for o in order..=MAX_ORDER {
            let Some(idx) = self.pop_free(o) else {
                continue;
            };
            for o in (order..o).rev() {
                let buddy_idx = idx ^ (1 << o);
                self.headers[buddy_idx].order = o as u8;
                self.push_free(o, buddy_idx);
            }
            self.headers[idx].order = order as u8;
            self.headers[idx].taken = true;
            return Some(idx);
        }
        None
    }

    #[allow(clippy::cast_possible_truncation)]
    fn dealloc(&mut self, mut idx: usize) {
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
        self.headers[idx].order = order as u8;
        self.headers[idx].taken = false;
        self.push_free(order, idx);
    }

    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let list = &mut self.free[order];
        list.head.take().inspect(|&head| {
            list.head = self.headers[head].next.take();
        })
    }

    fn push_free(&mut self, order: usize, idx: usize) {
        let list = &mut self.free[order];
        self.headers[idx].next = list.head.replace(idx);
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
}

#[derive(Default)]
struct Header {
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
