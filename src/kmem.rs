//! Sub-page level, malloc-like allocation system

use core::{
    alloc::{GlobalAlloc, Layout},
    fmt,
    mem::size_of,
    sync::atomic::{self, AtomicPtr},
};

use spin::{
    Once,
    mutex::{SpinMutex, SpinMutexGuard},
};

use crate::{
    align_value,
    frame::{self, FrameAllocator, FrameId},
    sv39::{PageTable, PhysAddr, VirtAddr},
};

/// Number of pages used for the kernel memory.
pub const PAGE_COUNT: usize = 64;

static KMEM: Once<SpinMutex<Allocator>> = Once::new();

/// Technically, we don't need the {} at the end, but it
/// reveals that we're creating a new structure and not just
/// copying a value.
#[global_allocator]
static ALLOCATOR: OsGlobalAlloc = OsGlobalAlloc;

/// If for some reason alloc() in the global allocator gets null_mut(),
/// then we come here. This is a divergent function, so we call panic to
/// let the tester know what's going on.
#[alloc_error_handler]
pub fn alloc_error(l: Layout) -> ! {
    panic!(
        "Allocator failed to allocate {} bytes with {}-byte alignment.",
        l.size(),
        l.align()
    );
}

/// Initialize the memory management system.
pub fn initialize() {
    KMEM.call_once(|| {
        SpinMutex::new(
            Allocator::new(frame::frame_allocator()).expect("kernel memory is allocated"),
        )
    });
}

/// Get a reference to the kernel memory.
pub fn kmem() -> SpinMutexGuard<'static, Allocator> {
    KMEM.get().expect("initialized kernel memory").lock()
}

/// Metadata for a region of byte-level allocation.
#[derive(Debug)]
pub struct AllocationNode(usize);

impl AllocationNode {
    /// Flag the current node as being taken.
    pub const FLAG_TAKEN: usize = 1 << 63;

    /// Return true if the node is taken.
    pub fn is_taken(&self) -> bool {
        self.0 & Self::FLAG_TAKEN != 0
    }

    /// Return true if the node is free.
    pub fn is_free(&self) -> bool {
        !self.is_taken()
    }

    /// Flag the node as being taken.
    pub fn take(&mut self) {
        self.0 |= Self::FLAG_TAKEN;
    }

    /// Clear the taken flag.
    pub fn free(&mut self) {
        self.0 &= !Self::FLAG_TAKEN;
    }

    /// Set the node size
    pub fn set_size(&mut self, size: usize) {
        let is_taken = self.is_taken();
        self.0 = size & !Self::FLAG_TAKEN;
        if is_taken {
            self.0 |= Self::FLAG_TAKEN;
        }
    }

    /// Get the node size
    pub fn get_size(&self) -> usize {
        self.0 & !Self::FLAG_TAKEN
    }
}

/// A linked list of nodes that manage the byte-level memory system.
pub struct AllocationList {
    head: AtomicPtr<u8>,
    tail: AtomicPtr<u8>,
}

impl AllocationList {
    /// Get the memory address of the list head.
    pub fn head(&self) -> usize {
        let head = self.head.load(atomic::Ordering::Relaxed);
        head as usize
    }

    /// Get the memory address of the list tail.
    pub fn tail(&self) -> usize {
        let tail = self.tail.load(atomic::Ordering::Relaxed);
        tail as usize
    }
}

impl fmt::Debug for AllocationList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for node_addr in self {
            let node = unsafe { &*node_addr };
            writeln!(
                f,
                "{:p}: Length = {:<10} Taken = {}",
                node_addr,
                node.get_size(),
                node.is_taken()
            )?;
        }
        Ok(())
    }
}

impl IntoIterator for &AllocationList {
    type Item = *const AllocationNode;

    type IntoIter = FreeAllocationListIter;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            ptr: self.head.load(atomic::Ordering::Relaxed),
            tail: self.tail.load(atomic::Ordering::Relaxed),
        }
    }
}

impl IntoIterator for &mut AllocationList {
    type Item = *mut AllocationNode;

    type IntoIter = FreeAllocationListIterMut;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            ptr: self.head.load(atomic::Ordering::Relaxed),
            tail: self.tail.load(atomic::Ordering::Relaxed),
        }
    }
}

/// An iterator going through the allocation node linked list.
#[derive(Debug)]
pub struct FreeAllocationListIter {
    tail: *const u8,
    ptr: *const u8,
}

impl Iterator for FreeAllocationListIter {
    type Item = *const AllocationNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr >= self.tail {
            return None;
        }
        let node_addr = self.ptr as *mut AllocationNode;
        let (node, ptr) = unsafe {
            let n = &*node_addr;
            let p = self.ptr.add(n.get_size());
            (n, p)
        };
        self.ptr = ptr;
        Some(node)
    }
}

/// A mutablel iterator going through the allocation node linked list.
#[derive(Debug)]
pub struct FreeAllocationListIterMut {
    tail: *mut u8,
    ptr: *mut u8,
}

impl Iterator for FreeAllocationListIterMut {
    type Item = *mut AllocationNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr >= self.tail {
            return None;
        }
        let node_addr = self.ptr as *mut AllocationNode;
        let (node, ptr) = unsafe {
            let n = &mut *node_addr;
            let p = self.ptr.add(n.get_size());
            (n, p)
        };
        self.ptr = ptr;
        Some(node)
    }
}

/// Metadata for the kernel's memory.
#[derive(Debug)]
pub struct Allocator {
    allocation_list: AllocationList,
    page_table_frame_id: FrameId,
}

impl Allocator {
    /// Initialize the kernel's memory.
    pub fn new(page_allocator: &FrameAllocator) -> Option<Self> {
        let kmem_head = page_allocator.zalloc(PAGE_COUNT)?;
        let kmem_tail = kmem_head + PAGE_COUNT + 1;
        let free_allocation = AllocationList {
            head: AtomicPtr::new(kmem_head.addr() as *mut u8),
            tail: AtomicPtr::new(kmem_tail.addr() as *mut u8),
        };
        let page_table_frame_id = page_allocator.zalloc(1)?;
        Some(Self {
            allocation_list: free_allocation,
            page_table_frame_id,
        })
    }

    /// Get a reference to the root page table.
    pub fn page_table_addr(&self) -> *mut PageTable {
        self.page_table_frame_id.addr() as *mut PageTable
    }

    /// Get a reference to the allocation list.
    pub fn allocation_list(&self) -> &AllocationList {
        &self.allocation_list
    }

    /// Allocate `size` bytes (8-byte aligned).
    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let size = align_value(size, 3) + size_of::<AllocationNode>();
        let mut allocation_list_iter = (&mut self.allocation_list).into_iter().peekable();
        while let Some(node_addr) = allocation_list_iter.next() {
            let node = unsafe { &mut *node_addr };
            let node_size = node.get_size();
            if node.is_free() && size <= node_size {
                node.take();
                let node_remaning = node_size - size;
                if node_remaning > size_of::<AllocationNode>() {
                    if let Some(&mut next_node_addr) = allocation_list_iter.peek_mut() {
                        let next_node = unsafe { &mut *next_node_addr };
                        next_node.free();
                        next_node.set_size(node_remaning);
                    }
                    node.set_size(size);
                } else {
                    node.set_size(node_size);
                }
                return Some(unsafe { (node_addr as *mut u8).add(1) });
            }
        }
        None
    }

    /// Allocate sub-page level allocation based on bytes and zero the memory
    pub fn zalloc(&mut self, size: usize) -> Option<*mut u8> {
        let addr = self.alloc(size)?;
        for i in 0..size {
            unsafe {
                (*addr.add(i)) = 0;
            }
        }
        Some(addr)
    }

    /// Deallocate the node starting at `ptr`.
    pub fn dealloc(&mut self, ptr: *mut u8) {
        if !ptr.is_null() {
            return;
        }
        let node = unsafe {
            let addr = (ptr as *mut AllocationNode).offset(-1);
            &mut *addr
        };
        if node.is_taken() {
            node.free();
        }
        self.coalesce();
    }

    /// Translates a virtual memory address into a physical one.
    pub fn virt2phys(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let table = unsafe { &*self.page_table_addr() };
        table.virt2phys(vaddr)
    }

    /// Merge smaller chunks into a bigger chunk
    pub fn coalesce(&mut self) {
        let mut allocation_list_iter = (&mut self.allocation_list).into_iter().peekable();
        while let Some(node_addr) = allocation_list_iter.next() {
            let node = unsafe { &mut *node_addr };
            if node.get_size() == 0 {
                break;
            }
            let next_node = match allocation_list_iter.peek_mut() {
                None => break,
                Some(&mut addr) => unsafe { &mut *addr },
            };
            if node.is_free() && next_node.is_free() {
                node.set_size(node.get_size() + next_node.get_size());
            }
        }
    }
}

// The global allocator is a static constant to a global allocator
// structure. We don't need any members because we're using this
// structure just to implement alloc and dealloc.
struct OsGlobalAlloc;

unsafe impl GlobalAlloc for OsGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // We align to the next page size so that when
        // we divide by PAGE_SIZE, we get exactly the number
        // of pages necessary.
        kmem().zalloc(layout.size()).unwrap()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // We ignore layout since our allocator uses ptr_start -> last
        // to determine the span of an allocation.
        kmem().dealloc(ptr);
    }
}
