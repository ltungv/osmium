use core::{
    alloc::{Layout, LayoutError},
    ptr::NonNull,
};

use crate::{addr, println};

fn align_up(ptr: *const u8, align: usize) -> *mut u8 {
    let addr = addr::align_up(ptr as usize, align);
    addr as *mut u8
}

// A simple byte-level allocator. Free memory blocks are kept track using a linked list whose nodes
// are backed by the same memory blocks that are being given out.
pub struct LinkedHeap {
    head: Node,
    info: Info,
}

impl LinkedHeap {
    /// Create an empty heap with no valid memory block.
    pub const fn empty() -> Self {
        Self {
            head: Node::dummy(),
            info: Info::new(core::ptr::null_mut(), 0),
        }
    }

    /// Return the address and size of the heap.
    pub const fn info(&self) -> Info {
        self.info
    }

    /// Initialize the heap given the address and size of a free memory block.
    pub unsafe fn init(&mut self, ptr: *mut u8, size: usize) {
        assert!(
            size >= size_of::<Node>(),
            "heap should have at least {} bytes",
            size_of::<Node>()
        );
        // Align the start of the heap to the alignment of a `Node`.
        let aligned_node_ptr = align_up(ptr, align_of::<Node>());
        // The heap start address is shifted up a few bytes after alignment.
        let aligned_offset = unsafe { aligned_node_ptr.offset_from_unsigned(ptr) };
        // Calculate the number of usable bytes after aligning the start address and size.
        let heap_len = addr::align_down(size - aligned_offset, align_of::<Node>());
        assert!(
            heap_len >= size_of::<Node>(),
            "heap should have at least {} bytes",
            size_of::<Node>()
        );
        self.info = Info::new(aligned_node_ptr, heap_len);
        self.head.next = Some(unsafe { Node::make(self.info, self.head.next.take()) });
    }

    pub fn debug_print(&self) {
        let mut next = self.head.next;
        while let Some(curr) = next {
            let curr = unsafe { curr.as_ref() };
            println!("{:p} - {} bytes", curr, curr.size);
            next = curr.next;
        }
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> Option<*mut u8> {
        let layout = Node::align_layout(layout).expect("layout should be aligned");
        let mut cursor = self.cursor()?;
        loop {
            match unsafe { cursor.split(layout) } {
                Ok(addr) => return Some(addr),
                Err(c) => {
                    cursor = c.next()?;
                }
            }
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let layout = Node::align_layout(layout).expect("layout should be aligned");
        let mut node = unsafe { Node::make(Info::new(ptr, layout.size()), None) };
        if let Some(cursor) = self.cursor() {
            let (cursor, merge_count) = match cursor.try_insert_precede(node) {
                Ok(cursor) => (cursor, 1),
                Err(mut cursor) => {
                    while !cursor.try_insert_succeed(node) {
                        cursor = cursor
                            .next()
                            .expect("cursor should not end before a list slot can be found");
                    }
                    (cursor, 2)
                }
            };
            cursor.try_merge(merge_count, self.info.end());
        } else {
            unsafe {
                node.as_mut().try_merge(self.info.end());
            }
            self.head.next = Some(node);
        }
    }

    fn cursor(&self) -> Option<Cursor> {
        self.head.next.map(|curr| Cursor {
            prev: NonNull::from(&self.head),
            curr,
        })
    }
}

#[repr(C)]
struct Node {
    next: Option<NonNull<Self>>,
    size: usize,
}

unsafe impl Send for Node {}

impl Node {
    const fn dummy() -> Self {
        Self {
            next: None,
            size: 0,
        }
    }

    unsafe fn make(info: Info, next: Option<NonNull<Self>>) -> NonNull<Self> {
        assert_eq!(
            info.ptr,
            align_up(info.ptr, align_of::<Self>()),
            "node should be aligned to {}",
            align_of::<Self>()
        );
        assert!(
            info.len >= size_of::<Self>(),
            "node should be at least {} bytes",
            size_of::<Self>()
        );
        unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let ptr = info.ptr.cast::<Self>();
            ptr.write(Self {
                size: info.len,
                next,
            });
            NonNull::new_unchecked(ptr)
        }
    }

    const fn info(&self) -> Info {
        Info::new(core::ptr::from_ref(self).cast_mut().cast(), self.size)
    }

    /// Adjust the layout such that the resulting memory block can also be used to store a `Node`.
    fn align_layout(layout: Layout) -> Result<Layout, LayoutError> {
        // When a memory block is free, a `Node` is stored at the beginning of the block to hold
        // the block's size and an optional pointer to the next free block. Once occupied, the
        // block is overwriten with the object described by the original layout.
        let size = layout.size().max(size_of::<Self>());
        let size = addr::align_up(size, align_of::<Self>());
        Layout::from_size_align(size, layout.align())
    }

    fn try_merge(&mut self, heap_end: *mut u8) {
        let node_end = self.info().end();
        if node_end < heap_end {
            let aligned_node_end = align_up(node_end, align_of::<Self>());
            let next_node_header_end = aligned_node_end.wrapping_add(size_of::<Self>());
            if next_node_header_end > heap_end {
                let offset = unsafe { heap_end.offset_from_unsigned(node_end) };
                self.size += offset;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Info {
    pub ptr: *mut u8,
    pub len: usize,
}

unsafe impl Send for Info {}

impl Info {
    const fn new(ptr: *mut u8, len: usize) -> Self {
        Self { ptr, len }
    }

    const fn end(&self) -> *mut u8 {
        self.ptr.wrapping_add(self.len)
    }

    fn split(self, layout: Layout) -> Option<(*mut u8, Option<Self>, Option<Self>)> {
        if self.len < layout.size() {
            return None;
        }
        let (alloc_addr, precede_info) = if self.ptr == align_up(self.ptr, layout.align()) {
            (self.ptr, None)
        } else {
            let shifted = self.ptr.wrapping_add(size_of::<Node>());
            let aligned = align_up(shifted, layout.align());
            let info = Self {
                ptr: self.ptr,
                len: unsafe { aligned.offset_from_unsigned(self.ptr) },
            };
            (aligned, Some(info))
        };
        let alloc_end = alloc_addr.wrapping_add(layout.size());
        let node_end = self.end();
        if alloc_end > node_end {
            return None;
        }
        let succeed_len = unsafe { node_end.offset_from_unsigned(alloc_end) };
        let succeed_info = if succeed_len == 0 {
            None
        } else {
            let addr = align_up(alloc_end, align_of::<Node>());
            let end = addr.wrapping_add(size_of::<Node>());
            if end > node_end {
                return None;
            }
            Some(Self {
                ptr: addr,
                len: succeed_len,
            })
        };
        Some((alloc_addr, precede_info, succeed_info))
    }
}

struct Cursor {
    prev: NonNull<Node>,
    curr: NonNull<Node>,
}

impl Cursor {
    fn next(mut self) -> Option<Self> {
        unsafe {
            self.curr.as_mut().next.map(|node| Self {
                prev: self.curr,
                curr: node,
            })
        }
    }

    unsafe fn split(mut self, layout: Layout) -> Result<*mut u8, Self> {
        let curr_info = unsafe { self.curr.as_ref().info() };
        let Some((alloc_addr, precede_info, succeed_info)) = curr_info.split(layout) else {
            return Err(self);
        };
        unsafe {
            self.prev.as_mut().next = None;
        }
        let next = unsafe { self.curr.as_mut().next.take() };
        match (precede_info, succeed_info) {
            (None, None) => unsafe {
                self.prev.as_mut().next = next;
            },
            (None, Some(info)) | (Some(info), None) => {
                let node = unsafe { Node::make(info, next) };
                unsafe {
                    self.prev.as_mut().next = Some(node);
                }
            }
            (Some(precede_info), Some(succeed_info)) => {
                let succeed_node = unsafe { Node::make(succeed_info, next) };
                let precede_node = unsafe { Node::make(precede_info, Some(succeed_node)) };
                unsafe {
                    self.prev.as_mut().next = Some(precede_node);
                }
            }
        }
        Ok(alloc_addr)
    }

    /// Try to insert a free node before the cursor's current node pointer.
    fn try_insert_precede(self, mut node: NonNull<Node>) -> Result<Self, Self> {
        if node >= self.curr {
            return Err(self);
        }
        let Self { mut prev, curr } = self;
        let node_info = unsafe { node.as_ref().info() };
        let curr_info = unsafe { curr.as_ref().info() };
        assert!(
            node_info.end() <= curr_info.ptr,
            "free nodes should not overlap"
        );
        unsafe {
            prev.as_mut().next = Some(node);
            node.as_mut().next = Some(self.curr);
        }
        Ok(Self { prev, curr: node })
    }

    /// Try to insert a free node after the cursor's current node pointer.
    fn try_insert_succeed(&mut self, mut node: NonNull<Node>) -> bool {
        let node_info = unsafe { node.as_ref().info() };
        if let Some(next) = unsafe { self.curr.as_ref().next } {
            if node >= next {
                return false;
            }
            assert!(
                node_info.end() <= next.as_ptr().cast::<u8>(),
                "free nodes should not overlap"
            );
        }
        debug_assert!(self.curr < node, "list should be in order");
        let curr_info = unsafe { self.curr.as_ref().info() };
        assert!(
            curr_info.end() <= node_info.ptr,
            "free nodes should not overlap"
        );
        unsafe {
            node.as_mut().next = self.curr.as_mut().next.replace(node);
        }
        true
    }

    fn try_merge(self, count: usize, heap_end: *mut u8) {
        let Self { mut curr, .. } = self;
        for _ in 0..count {
            let Some(mut next) = (unsafe { curr.as_ref().next }) else {
                unsafe {
                    curr.as_mut().try_merge(heap_end);
                }
                return;
            };
            let curr_info = unsafe { curr.as_ref().info() };
            if curr_info.end() != next.as_ptr().cast::<u8>() {
                curr = next;
                continue;
            }
            unsafe {
                curr.as_mut().next = next.as_mut().next.take();
                curr.as_mut().size += next.as_ref().size;
            }
        }
    }
}
