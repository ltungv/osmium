use core::{
    alloc::{GlobalAlloc, Layout},
    ptr,
};

use crate::addr::align_up;

pub struct LinkedHeap {
    head: Node,
}

impl LinkedHeap {
    pub const fn new() -> Self {
        Self { head: Node::new(0) }
    }

    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.push(heap_start, heap_size);
        }
    }

    fn find(&mut self, size: usize, align: usize) -> Option<(&'static mut Node, usize)> {
        let mut prev = &mut self.head;
        while let Some(curr) = prev.next.as_mut() {
            if let Some(alloc_start) = Self::alloc(curr, size, align) {
                let next = curr.next.take();
                let ret = (prev.next.take().unwrap(), alloc_start);
                prev.next = next;
                return Some(ret);
            }
            prev = prev.next.as_mut().unwrap();
        }
        None
    }

    unsafe fn push(&mut self, addr: usize, size: usize) {
        assert_eq!(
            align_up(addr, align_of::<Node>()),
            addr,
            "address should be aligned to {}",
            align_of::<Node>()
        );
        assert!(
            size >= size_of::<Node>(),
            "size should be at least {}",
            size_of::<Node>()
        );

        let mut node = Node::new(size);
        node.next = self.head.next.take();

        let node_ptr = addr as *mut Node;
        unsafe {
            node_ptr.write(node);
            self.head.next = Some(&mut *node_ptr);
        }
    }

    fn alloc(node: &Node, size: usize, align: usize) -> Option<usize> {
        let alloc_start = align_up(node.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size)?;
        if alloc_end > node.end_addr() {
            return None;
        }
        let rem = node.end_addr() - alloc_end;
        if rem > 0 && rem < size_of::<Node>() {
            return None;
        }
        Some(alloc_start)
    }
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(align_of::<Node>())
            .expect("adjusting alignment failed")
            .pad_to_align();

        let size = layout.size().max(size_of::<Node>());
        (size, layout.align())
    }
}

struct Node {
    size: usize,
    next: Option<&'static mut Node>,
}

impl Node {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }

    fn start_addr(&self) -> usize {
        core::ptr::from_ref::<Self>(self) as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct LockedLinkedHeap(spin::Mutex<LinkedHeap>);

impl LockedLinkedHeap {
    pub const fn new() -> Self {
        Self(spin::Mutex::new(LinkedHeap::new()))
    }
}

unsafe impl GlobalAlloc for LockedLinkedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedHeap::size_align(layout);
        let mut allocator = self.0.lock();
        if let Some((region, alloc_start)) = allocator.find(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                unsafe {
                    allocator.push(alloc_end, excess_size);
                }
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedHeap::size_align(layout);
        unsafe { self.0.lock().push(ptr as usize, size) }
    }
}
