use core::{fmt, slice};

use crate::{
    HEAP_ADDR, MEM_ADDR, MEM_SIZE, PAGE_SIZE,
    addr::{PhysAddr, PhysPageNumber, VirtAddr},
};

const MAX_ORDER: usize = 12;

static KALLOC: spin::Mutex<BuddyAlloc> = spin::Mutex::new(BuddyAlloc::empty());

pub fn init() {
    let mut kalloc = KALLOC.lock();
    unsafe {
        kalloc.init(
            PhysAddr::new_trunc(HEAP_ADDR),
            (MEM_ADDR + MEM_SIZE) - HEAP_ADDR,
        );
    }
}

pub fn get() -> &'static spin::Mutex<BuddyAlloc> {
    &KALLOC
}

pub struct BuddyAlloc {
    addr: PhysPageNumber,
    headers: &'static mut [Header],
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
    const fn empty() -> Self {
        Self {
            addr: PhysPageNumber::new_trunc(0),
            headers: &mut [],
            free_list: [const { None }; MAX_ORDER + 1],
        }
    }

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

    pub fn alloc(&mut self, num_pages: usize) -> Option<PhysPageNumber> {
        let order = num_pages.next_power_of_two().highest_one()? as usize;
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

    pub fn dealloc(&mut self, ppn: PhysPageNumber) {
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

    #[expect(clippy::cast_possible_truncation)]
    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let next = &mut self.free_list[order];
        next.take().inspect(|&idx| {
            *next = self.headers[idx].next.take();
            self.headers[idx].order = order as u8;
            self.headers[idx].taken = true;
        })
    }

    #[expect(clippy::cast_possible_truncation)]
    const fn push_free(&mut self, order: usize, idx: usize) {
        let next = &mut self.free_list[order];
        self.headers[idx].next = next.replace(idx);
        self.headers[idx].order = order as u8;
        self.headers[idx].taken = false;
    }

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

#[derive(Default, Debug)]
struct Header {
    next: Option<usize>,
    order: u8,
    taken: bool,
}

impl Header {
    fn slice_from_ppn_mut(addr: PhysAddr, len: usize) -> &'static mut [Self] {
        let headers_ptr = unsafe { VirtAddr::direct(addr).as_ptr_mut::<Self>() };
        for i in 0..len {
            unsafe {
                headers_ptr.add(i).write(Self::default());
            }
        }
        unsafe { slice::from_raw_parts_mut(headers_ptr, len) }
    }
}
