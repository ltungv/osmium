use core::{alloc::Layout, ptr};

use crate::{
    addr::{align_up, phys_to_virt},
    mem::ppn::PhysPageNumber,
    println,
};

pub struct LinkedHeap {
    head: Node,
    addr: usize,
    size: usize,
}

impl LinkedHeap {
    pub const fn new() -> Self {
        Self {
            head: Node::new(0),
            addr: 0,
            size: 0,
        }
    }

    pub unsafe fn init(&mut self, ppn: PhysPageNumber, size: usize) -> bool {
        self.addr = unsafe { phys_to_virt(ppn.addr()).as_ptr_mut::<u8>() as usize };
        self.size = size;
        unsafe {
            self.push(self.addr, self.size);
        }
        true
    }

    pub const fn start_addr(&self) -> usize {
        self.addr
    }

    pub const fn end_addr(&self) -> usize {
        self.addr + self.size
    }

    pub fn debug_print(&self) {
        let mut next = &self.head.next;
        while let Some(curr) = next {
            println!("{:p} - {} bytes", curr, curr.size);
            next = &curr.next;
        }
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);
        let Some((region, alloc_start)) = self.find(size, align) else {
            return ptr::null_mut();
        };

        let alloc_end = alloc_start
            .checked_add(size)
            .expect("allocation should not overflow");

        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 {
            unsafe {
                self.push(alloc_end, excess_size);
            }
        }
        alloc_start as *mut u8
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        unsafe { self.push(ptr as usize, size) }
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

    fn find(&mut self, size: usize, align: usize) -> Option<(&'static mut Node, usize)> {
        let mut prev = &mut self.head;
        while let Some(curr) = prev.next.as_mut() {
            if let Some(alloc_start) = Self::try_take_node(curr, size, align) {
                let next = curr.next.take();
                let ret = (prev.next.take().unwrap(), alloc_start);
                prev.next = next;
                return Some(ret);
            }
            prev = prev.next.as_mut().unwrap();
        }
        None
    }

    fn try_take_node(node: &Node, size: usize, align: usize) -> Option<usize> {
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
            .expect("layout should be aligned")
            .pad_to_align();

        let size = layout.size().max(size_of::<Node>());
        (size, layout.align())
    }
}

struct Node {
    size: usize,
    next: Option<&'static mut Self>,
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
