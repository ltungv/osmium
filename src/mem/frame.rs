use core::slice;

use crate::{
    PAGE_SIZE,
    addr::{PhysAddr, phys_to_virt},
    mem::ppn::PhysPageNumber,
    println,
};

const MAX_ORDER: usize = 12;

pub struct BuddyAlloc {
    addr: PhysPageNumber,
    headers: &'static mut [Header],
    free_list: [Link; MAX_ORDER + 1],
}

impl BuddyAlloc {
    pub const fn new() -> Self {
        Self {
            addr: PhysPageNumber::new_trunc(0),
            headers: &mut [],
            free_list: [const { Link::new() }; MAX_ORDER + 1],
        }
    }

    pub unsafe fn init(&mut self, addr: PhysAddr, len: usize) {
        let header_start_addr = addr.align_up(align_of::<Header>());
        let useable_len = addr + len - header_start_addr;
        let mut unprovisioned_frames = useable_len / (size_of::<Header>() + PAGE_SIZE);

        self.headers = Header::slice_from_ppn_mut(header_start_addr, unprovisioned_frames);
        self.addr = (header_start_addr + size_of_val(self.headers))
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

    pub fn alloc(&mut self, order: usize) -> Option<PhysPageNumber> {
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
        assert!(ppn >= self.addr);
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

    pub fn debug_print(&self) {
        println!("BuddyAllocator");
        println!(
            "frames: {:p} -> {:p} ({} frames, {} KB)",
            self.addr,
            self.addr + self.headers.len(),
            self.headers.len(),
            PAGE_SIZE * self.headers.len() / 1024,
        );
        println!("free: {{");
        let mut total_free_frames = 0;
        for order in (0..=MAX_ORDER).rev() {
            let mut free_blocks = 0;
            let mut next = self.free_list[order].next;
            while let Some(curr) = next {
                free_blocks += 1;
                next = self.headers[curr].link.next;
            }
            let free_frames = free_blocks * (1 << order);
            total_free_frames += free_frames;
            println!(
                "  [{order:>02}] {free_blocks} blocks, {free_frames} frames, {} KB",
                PAGE_SIZE * free_frames / 1024
            );
        }
        println!(
            "}} {} frames, {} KB",
            total_free_frames,
            PAGE_SIZE * total_free_frames / 1024
        );
    }

    #[allow(clippy::cast_possible_truncation)]
    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let link = &mut self.free_list[order];
        link.next.take().inspect(|&idx| {
            link.next = self.headers[idx].link.next.take();
            self.headers[idx].order = order as u8;
            self.headers[idx].taken = true;
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    fn push_free(&mut self, order: usize, idx: usize) {
        let link = &mut self.free_list[order];
        self.headers[idx].link.next = link.next.replace(idx);
        self.headers[idx].order = order as u8;
        self.headers[idx].taken = false;
    }

    fn remove_free(&mut self, order: usize, idx: usize) {
        let mut prev: Option<usize> = None;
        let mut next = self.free_list[order].next;
        while let Some(curr_idx) = next {
            if curr_idx == idx {
                if let Some(prev_idx) = prev {
                    self.headers[prev_idx].link.next = self.headers[curr_idx].link.next.take();
                } else {
                    self.free_list[order].next = self.headers[curr_idx].link.next.take();
                }
                break;
            }
            prev = Some(curr_idx);
            next = self.headers[curr_idx].link.next;
        }
    }
}

#[derive(Default)]
struct Header {
    link: Link,
    order: u8,
    taken: bool,
}

impl Header {
    fn slice_from_ppn_mut(addr: PhysAddr, len: usize) -> &'static mut [Self] {
        let headers_ptr = unsafe { phys_to_virt(addr).as_ptr_mut::<Self>() };
        for i in 0..len {
            unsafe {
                headers_ptr.add(i).write(Self::default());
            }
        }
        unsafe { slice::from_raw_parts_mut(headers_ptr, len) }
    }
}

#[derive(Default)]
struct Link {
    next: Option<usize>,
}

impl Link {
    const fn new() -> Self {
        Self { next: None }
    }
}
