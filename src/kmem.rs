//! Sub-page level, malloc-like allocation system

use core::{
    alloc::{GlobalAlloc, Layout},
    fmt,
    mem::size_of,
};

use spin::{
    Once,
    mutex::{SpinMutex, SpinMutexGuard},
};

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START, align_value,
    frame::{self, FRAME_SIZE, FrameAllocator, FrameId},
    sv39::{self, EntryFlags, PageTable, PhysAddr, VirtAddr},
    uart,
};

/// Number of pages used for the kernel memory.
pub const PAGE_COUNT: usize = 64;

static KMEM: Once<SpinMutex<Allocator>> = Once::new();

/// Initialize the memory management system.
pub fn initialize() {
    KMEM.call_once(|| {
        let alloc = Allocator::new(frame::frame_allocator()).expect("kernel memory is allocated");
        alloc.identity_map().expect("kernel memory is mapped");
        SpinMutex::new(alloc)
    });
}

/// Get a reference to the kernel memory.
pub fn kmem() -> SpinMutexGuard<'static, Allocator> {
    KMEM.get().expect("initialized kernel memory").lock()
}

#[global_allocator]
static ALLOCATOR: OsGlobalAlloc = OsGlobalAlloc;

#[alloc_error_handler]
fn alloc_error(l: Layout) -> ! {
    panic!(
        "Allocator failed to allocate {} bytes with {}-byte alignment.",
        l.size(),
        l.align()
    );
}

/// Metadata for a region of byte-level allocation.
#[derive(Debug, Default)]
pub struct AllocationNode(usize);

impl AllocationNode {
    /// Flag the current node as being taken.
    pub const TAKEN_FLAG_MASK: usize = 1 << 63;

    /// Returns an immutable raw pointer to the next allocation node.
    ///
    /// # Safety
    ///
    /// The pointer to the current allocation node, `*mut Self`, must be valid and points to an
    /// initialized `AllocationNode` that correctly represents the size of the allocated region.
    pub unsafe fn next(ptr: *const Self) -> *const Self {
        let node = unsafe { &*ptr };
        let ptr = ptr.cast::<u8>();
        let ptr = unsafe { ptr.add(node.get_size()) };
        ptr.cast::<AllocationNode>()
    }

    /// Returns a mutable raw pointer to the next allocation node.
    ///
    /// # Safety
    ///
    /// The pointer to the current allocation node, `*mut Self`, must be valid and points to an
    /// initialized `AllocationNode` that correctly represents the size of the allocated region.
    pub unsafe fn next_mut(ptr: *mut Self) -> *mut Self {
        let node = unsafe { &*ptr };
        let ptr = ptr.cast::<u8>();
        let ptr = unsafe { ptr.add(node.get_size()) };
        ptr.cast::<AllocationNode>()
    }

    /// Clear the taken flag.
    pub fn free(&mut self) {
        self.0 &= !Self::TAKEN_FLAG_MASK;
    }

    /// Return true if the node is free.
    pub fn is_free(&self) -> bool {
        self.0 & Self::TAKEN_FLAG_MASK == 0
    }

    /// Set the taken flag.
    pub fn take(&mut self) {
        self.0 |= Self::TAKEN_FLAG_MASK;
    }

    /// Return true if the node is taken.
    pub fn is_taken(&self) -> bool {
        self.0 & Self::TAKEN_FLAG_MASK != 0
    }

    /// Set the node size.
    pub fn set_size(&mut self, size: usize) {
        let is_taken = self.is_taken();
        self.0 = size & !Self::TAKEN_FLAG_MASK;
        if is_taken {
            self.0 |= Self::TAKEN_FLAG_MASK;
        }
    }

    /// Get the node size.
    pub fn get_size(&self) -> usize {
        self.0 & !Self::TAKEN_FLAG_MASK
    }
}

/// A linked list of nodes that manage the byte-level memory system.
pub struct AllocationList {
    head: FrameId,
    tail: FrameId,
}

impl AllocationList {
    /// Get the memory address of the list head.
    pub fn head(&self) -> usize {
        self.head.addr()
    }

    /// Get the memory address of the list tail.
    pub fn tail(&self) -> usize {
        self.tail.addr()
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

    type IntoIter = AllocatorListIter;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            ptr: self.head.addr() as *const u8,
            tail: self.tail.addr() as *const u8,
        }
    }
}

impl IntoIterator for &mut AllocationList {
    type Item = *mut AllocationNode;

    type IntoIter = AllocationListIterMut;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            ptr: self.head.addr() as *mut u8,
            tail: self.tail.addr() as *mut u8,
        }
    }
}

/// An iterator going through the allocation node linked list.
#[derive(Debug)]
pub struct AllocatorListIter {
    ptr: *const u8,
    tail: *const u8,
}

impl Iterator for AllocatorListIter {
    type Item = *const AllocationNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr >= self.tail {
            return None;
        }
        let ptr = self.ptr.cast::<AllocationNode>();
        self.ptr = unsafe { AllocationNode::next(ptr).cast() };
        Some(ptr)
    }
}

/// A mutablel iterator going through the allocation node linked list.
#[derive(Debug)]
pub struct AllocationListIterMut {
    ptr: *mut u8,
    tail: *mut u8,
}

impl Iterator for AllocationListIterMut {
    type Item = *mut AllocationNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr >= self.tail {
            return None;
        }
        let ptr = self.ptr.cast::<AllocationNode>();
        self.ptr = unsafe { AllocationNode::next_mut(ptr).cast() };
        Some(ptr)
    }
}

/// Metadata for the kernel's memory.
pub struct Allocator {
    alloc_list: AllocationList,
    root_frame_id: FrameId,
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "root_frame_id = {:?}", self.root_frame_id)?;
        self.alloc_list.fmt(f)
    }
}

impl Allocator {
    /// Initialize the kernel's memory.
    pub fn new(frame_allocator: &FrameAllocator) -> Option<Self> {
        let head = frame_allocator.zalloc(PAGE_COUNT)?;
        let tail = head + PAGE_COUNT;
        let alloc_list = AllocationList { head, tail };

        let node = unsafe {
            let ptr = head.addr() as *mut AllocationNode;
            &mut *ptr
        };
        *node = AllocationNode::default();
        node.free();
        node.set_size(FRAME_SIZE * PAGE_COUNT);

        let root_frame_id = frame_allocator.zalloc(1)?;
        Some(Self {
            alloc_list,
            root_frame_id,
        })
    }

    /// Returns the first and last memory address of the kernel.
    pub fn mem_region(&self) -> (usize, usize) {
        (self.alloc_list.head(), self.alloc_list.tail())
    }

    /// Returns the identification of the root frame of the kernel.
    pub fn root_frame_id(&self) -> FrameId {
        self.root_frame_id
    }

    /// Allocate `size` bytes (8-byte aligned).
    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let size = align_value(size, 3) + size_of::<AllocationNode>();
        for node_ptr in &mut self.alloc_list {
            let node = unsafe { &mut *node_ptr };
            let node_size = node.get_size();
            if node.is_free() && size <= node_size {
                node.take();
                let node_remaning = node_size - size;
                if node_remaning > size_of::<AllocationNode>() {
                    node.set_size(size);
                    let next_node_ptr = unsafe { AllocationNode::next_mut(node_ptr) };
                    let next_node = unsafe { &mut *next_node_ptr };
                    next_node.free();
                    next_node.set_size(node_remaning);
                } else {
                    node.set_size(node_size);
                }
                return Some(unsafe { node_ptr.add(1).cast() });
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
        if ptr.is_null() {
            return;
        }
        let node = unsafe {
            let addr = ptr.cast::<AllocationNode>().offset(-1);
            &mut *addr
        };
        if node.is_taken() {
            node.free();
        }
        self.coalesce();
    }

    /// Translates a virtual memory address into a physical one.
    pub fn virt2phys(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let table_ptr = self.root_frame_id().addr() as *mut PageTable;
        let table = unsafe { &*table_ptr };
        table.virt2phys(vaddr)
    }

    /// Merge smaller chunks into a bigger chunk
    pub fn coalesce(&mut self) {
        let mut ptr = self.alloc_list.head() as *mut u8;
        let tail = self.alloc_list.tail() as *mut u8;
        while ptr < tail {
            let node = unsafe { &mut *ptr.cast::<AllocationNode>() };
            let size = node.get_size();
            if size == 0 {
                break;
            }
            let next_ptr = unsafe { ptr.add(size) };
            if next_ptr >= tail {
                break;
            }
            let next_node = unsafe { &mut *next_ptr.cast::<AllocationNode>() };
            if node.is_free() && next_node.is_free() {
                node.set_size(size + next_node.get_size());
            } else {
                ptr = next_ptr;
            }
        }
    }

    /// Identity map all sections of the kernel's memory.
    fn identity_map(&self) -> Result<(), sv39::Error> {
        let (kmem_start, kmem_end) = self.mem_region();
        let table_ptr = self.root_frame_id().addr() as *mut PageTable;
        let root = unsafe { &mut *table_ptr };

        root.map(
            frame::frame_allocator(),
            uart::BASE_ADDRESS.into(),
            uart::BASE_ADDRESS.into(),
            EntryFlags::READ | EntryFlags::WRITE,
            0,
        )?;

        root.id_map_range(
            frame::frame_allocator(),
            kmem_start,
            kmem_end,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;

        unsafe {
            root.id_map_range(
                frame::frame_allocator(),
                HEAP_START,
                HEAP_START + HEAP_SIZE,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root.id_map_range(
                frame::frame_allocator(),
                TEXT_START,
                TEXT_END,
                EntryFlags::READ | EntryFlags::EXECUTE,
            )?;

            root.id_map_range(
                frame::frame_allocator(),
                RODATA_START,
                RODATA_END,
                EntryFlags::READ | EntryFlags::EXECUTE,
            )?;

            root.id_map_range(
                frame::frame_allocator(),
                DATA_START,
                DATA_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root.id_map_range(
                frame::frame_allocator(),
                BSS_START,
                BSS_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root.id_map_range(
                frame::frame_allocator(),
                KERNEL_STACK_START,
                KERNEL_STACK_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;
        }
        Ok(())
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
