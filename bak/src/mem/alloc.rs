//! Sub-page level, malloc-like allocation system

use core::{
    alloc::{GlobalAlloc, Layout},
    fmt,
    mem::size_of,
    sync::atomic::{self, AtomicPtr},
};

use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::{
    align_value,
    mem::{PageTable, PAGE_SIZE},
};

use super::PageAllocator;

/// Number of pages used for the kernel memory.
pub const KMEM_PAGES: usize = 64;

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

static OS_GLOBAL_KMEM: spin::Once<SpinMutex<KernelMemory>> = spin::Once::new();

/// Initialize the kernel memory.
pub fn initialize(page_allocator: &mut PageAllocator) {
    OS_GLOBAL_KMEM.call_once(|| {
        let mem = KernelMemory::new(page_allocator).expect("Could not allocate kernel memory.");
        SpinMutex::new(mem)
    });
}

/// Get a reference to the kernel memory.
pub fn kmem() -> SpinMutexGuard<'static, KernelMemory> {
    OS_GLOBAL_KMEM.get().expect("Invalid state.").lock()
}

/// Metadata for the kernel's memory.
#[derive(Debug)]
pub struct KernelMemory {
    allocation_list: AllocationList,
    page_table: AtomicPtr<PageTable>,
}

impl KernelMemory {
    /// Initialize the kernel's memory.
    fn new(page_allocator: &mut PageAllocator) -> Option<Self> {
        let kernel_pages_addr = page_allocator.zalloc(KMEM_PAGES)?;
        let free_allocation = AllocationList {
            head: AtomicPtr::new(kernel_pages_addr),
            tail: AtomicPtr::new(unsafe { kernel_pages_addr.add(KMEM_PAGES * PAGE_SIZE) }),
        };
        let page_table_addr = page_allocator.zalloc(1)?;
        Some(Self {
            allocation_list: free_allocation,
            page_table: AtomicPtr::new(page_table_addr as *mut PageTable),
        })
    }

    /// Get a reference to the root page table.
    pub fn page_table_addr(&mut self) -> *mut PageTable {
        self.page_table.load(atomic::Ordering::Relaxed)
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
        let mut kmem = kmem();
        kmem.zalloc(layout.size()).unwrap();
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // We ignore layout since our allocator uses ptr_start -> last
        // to determine the span of an allocation.
        let mut kmem = kmem();
        kmem.dealloc(ptr);
        todo!()
    }
}

#[global_allocator]
/// Technically, we don't need the {} at the end, but it
/// reveals that we're creating a new structure and not just
/// copying a value.
static GA: OsGlobalAlloc = OsGlobalAlloc;

#[alloc_error_handler]
/// If for some reason alloc() in the global allocator gets null_mut(),
/// then we come here. This is a divergent function, so we call panic to
/// let the tester know what's going on.
pub fn alloc_error(l: Layout) -> ! {
    panic!(
        "Allocator failed to allocate {} bytes with {}-byte alignment.",
        l.size(),
        l.align()
    );
}
