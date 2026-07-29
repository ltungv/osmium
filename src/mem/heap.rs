//! Sub-page level, malloc-like allocation system

use core::{fmt, marker::PhantomData, mem::size_of, ptr::NonNull};

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START, align_value,
    mem::{
        PAGE_SIZE,
        frame::{self, FrameNumber},
        frame_allocator,
        page::{self, EntryFlags, PageTable},
    },
    uart,
};

/// Number of pages used for the kernel heap allocator.
pub(crate) const PAGE_COUNT: usize = 64;

/// Metadata for the kernel's memory.
pub(crate) struct Allocator {
    alloc_list: AllocationList,
    root_frame_id: FrameNumber,
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "root_frame @ 0x{:x}", self.root_frame_id.addr())?;
        self.alloc_list.fmt(f)
    }
}

impl Allocator {
    /// Initialize the kernel's memory.
    pub(crate) fn new(frame_allocator: &frame::Allocator) -> Option<Self> {
        let head_frame = frame_allocator.zalloc(PAGE_COUNT)?;
        let tail_frame = head_frame + PAGE_COUNT;

        // SAFETY: `head_frame.addr()` is the start of a freshly zero-allocated
        // region of `PAGE_COUNT` frames. Writing an `AllocationNode` at this
        // address is valid because the region is large enough and the address
        // is 4096-byte aligned (satisfies `AllocationNode`'s `usize` alignment).
        let mut head = unsafe {
            let ptr = head_frame.addr() as *mut AllocNode;
            NodePtr::from_raw(ptr)
        };
        let tail = tail_frame.addr() as *const u8;

        {
            let node = head.as_mut();
            *node = AllocNode::default();
            node.free();
            node.set_size(PAGE_SIZE * PAGE_COUNT);
        }

        let alloc_list = AllocationList { head, tail };
        let root_frame_id = frame_allocator.zalloc(1)?;

        Some(Self {
            alloc_list,
            root_frame_id,
        })
    }

    /// Returns the first and last memory address of the kernel.
    pub(crate) fn mem_region(&self) -> (usize, usize) {
        (self.alloc_list.head_addr(), self.alloc_list.tail_addr())
    }

    /// Returns the identification of the root frame of the kernel.
    pub(crate) fn root_frame_id(&self) -> FrameNumber {
        self.root_frame_id
    }

    /// Allocate `size` bytes (8-byte aligned).
    pub(crate) fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let size = align_value(size, 3) + size_of::<AllocNode>();
        let tail = self.alloc_list.tail;

        for mut node_ptr in self.alloc_list.iter_nodes() {
            let node = node_ptr.as_mut();
            let node_size = node.get_size();
            if node.is_free() && size <= node_size {
                node.take();
                let node_remaining = node_size - size;
                if node_remaining > size_of::<AllocNode>() {
                    node.set_size(size);
                    // Splitting: initialise the remainder as a free node.
                    if let Some(mut next) = node_ptr.next(tail) {
                        let next_node = next.as_mut();
                        next_node.free();
                        next_node.set_size(node_remaining);
                    }
                } else {
                    node.set_size(node_size);
                }
                return Some(node_ptr.user_ptr());
            }
        }
        None
    }

    /// Allocate sub-page level allocation based on bytes and zero the memory.
    pub(crate) fn zalloc(&mut self, size: usize) -> Option<*mut u8> {
        let addr = self.alloc(size)?;
        // SAFETY: `addr` points to `size` bytes of usable payload inside the
        // heap region, as returned by `alloc` above.
        unsafe {
            core::ptr::write_bytes(addr, 0, size);
        }
        Some(addr)
    }

    /// Deallocate the node starting at `ptr`.
    pub(crate) fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        // SAFETY: `ptr` was returned by a prior `alloc`/`zalloc` and has not
        // been deallocated yet — the caller (`GlobalAlloc::dealloc`) guarantees
        // this per its own safety contract.
        let mut node_ptr = unsafe { NodePtr::from_user_ptr(ptr) };
        let node = node_ptr.as_mut();
        if node.is_taken() {
            node.free();
        }
        self.coalesce();
    }

    /// Translates a virtual memory address into a physical one.
    pub(crate) fn virt2phys(&self, vaddr: usize) -> Option<usize> {
        let table_ptr = self.root_frame_id().addr() as *const PageTable;
        // SAFETY: `root_frame_id` was allocated via `zalloc(1)` in `new` and
        // is valid for the lifetime of the `Allocator`.
        let table = unsafe { &*table_ptr };
        table.virt2phys(vaddr)
    }

    /// Merge adjacent free chunks into a bigger chunk.
    pub(crate) fn coalesce(&mut self) {
        let tail = self.alloc_list.tail;
        let mut current = Some(self.alloc_list.head);

        while let Some(mut node_ptr) = current {
            // Extract data from the mutable borrow before using `node_ptr`
            // again (for `.next()`), to avoid overlapping borrows.
            let size = node_ptr.as_ref().get_size();
            let is_free = node_ptr.as_ref().is_free();
            if size == 0 {
                break;
            }
            let Some(next_ptr) = node_ptr.next(tail) else {
                break;
            };
            if is_free && next_ptr.as_ref().is_free() {
                let next_size = next_ptr.as_ref().get_size();
                node_ptr.as_mut().set_size(size + next_size);
                // Don't advance — the merged node may coalesce further.
                current = Some(node_ptr);
            } else {
                current = Some(next_ptr);
            }
        }
    }

    /// Identity map all sections of the kernel's memory.
    pub(crate) fn identity_map(&self) -> Result<(), page::Error> {
        let (kmem_start, kmem_end) = self.mem_region();
        let root_table_ptr = self.root_frame_id().addr() as *mut PageTable;

        // SAFETY: `root_frame_id` was allocated via `zalloc(1)` in `new` and
        // is valid for the lifetime of the `Allocator`. No other code mutates
        // this page table concurrently (called during single-threaded init).
        let root_table = unsafe { &mut *root_table_ptr };

        root_table.map(
            frame_allocator(),
            uart::BASE_ADDRESS,
            uart::BASE_ADDRESS,
            EntryFlags::READ | EntryFlags::WRITE,
            0,
        )?;

        root_table.id_map_range(
            frame_allocator(),
            kmem_start,
            kmem_end,
            EntryFlags::READ | EntryFlags::WRITE,
        )?;

        // SAFETY: the linker-script symbols below are valid addresses
        // provided by the linker and represent the kernel's memory layout.
        unsafe {
            root_table.id_map_range(
                frame_allocator(),
                HEAP_START,
                HEAP_START + HEAP_SIZE,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root_table.id_map_range(
                frame_allocator(),
                TEXT_START,
                TEXT_END,
                EntryFlags::READ | EntryFlags::EXECUTE,
            )?;

            root_table.id_map_range(
                frame_allocator(),
                RODATA_START,
                RODATA_END,
                EntryFlags::READ | EntryFlags::EXECUTE,
            )?;

            root_table.id_map_range(
                frame_allocator(),
                DATA_START,
                DATA_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root_table.id_map_range(
                frame_allocator(),
                BSS_START,
                BSS_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;

            root_table.id_map_range(
                frame_allocator(),
                KERNEL_STACK_START,
                KERNEL_STACK_END,
                EntryFlags::READ | EntryFlags::WRITE,
            )?;
        }
        Ok(())
    }
}

/// A contiguous sequence of allocation nodes spanning the kernel heap region.
///
/// `head` points to the first `AllocationNode`. `tail` is a one-past-end
/// sentinel (never dereferenced) used to stop iteration.
struct AllocationList {
    head: NodePtr,
    tail: *const u8,
}

// SAFETY: `AllocationList` contains a `NodePtr` (see its `Send` impl) and a
// `*const u8` tail sentinel that is never dereferenced — only compared.
// The underlying heap memory is `'static` and access is serialised by the
// `SpinMutex` that guards `Allocator`.
unsafe impl Send for AllocationList {}

impl AllocationList {
    /// Get the memory address of the list head.
    pub(crate) fn head_addr(&self) -> usize {
        self.head.as_raw() as usize
    }

    /// Get the memory address of the list tail.
    pub(crate) fn tail_addr(&self) -> usize {
        self.tail as usize
    }

    /// Return an iterator over all nodes in the list.
    fn iter_nodes(&self) -> NodeIter<'_> {
        NodeIter {
            curr: Some(self.head),
            tail: self.tail,
            _phantom: PhantomData,
        }
    }
}

impl fmt::Debug for AllocationList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for node_ptr in self.iter_nodes() {
            let node = node_ptr.as_ref();
            writeln!(
                f,
                "{:p}: Length = {:<10} Taken = {}",
                node_ptr.as_raw(),
                node.get_size(),
                node.is_taken()
            )?;
        }
        Ok(())
    }
}

/// An iterator over the allocation nodes in an [`AllocationList`].
///
/// The `PhantomData<&'a AllocationList>` borrows the list so the iterator
/// cannot outlive the list it was created from.
struct NodeIter<'a> {
    curr: Option<NodePtr>,
    tail: *const u8,
    _phantom: PhantomData<&'a AllocationList>,
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = NodePtr;

    fn next(&mut self) -> Option<Self::Item> {
        let node_ptr = self.curr?;
        self.curr = node_ptr.next(self.tail);
        Some(node_ptr)
    }
}

/// A non-null pointer to an [`AllocationNode`] within the kernel heap region.
///
/// It's unsafe to construct a `NodePtr` and it must be ensure that every `NodePtr`
/// points to a valid, aligned, `&'static AllocationNode` inside the heap region.
#[derive(Clone, Copy)]
struct NodePtr(NonNull<AllocNode>);

// SAFETY: `NodePtr` wraps a `NonNull<AllocationNode>` that always points into
// the kernel heap - a `'static` memory region that is never moved or freed.
// Access is synchronised by the `SpinMutex` that guards `Allocator`.
unsafe impl Send for NodePtr {}

impl NodePtr {
    /// Create a `NodePtr` from a raw pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null, properly aligned for `AllocationNode`, and
    /// point to a valid, initialized `AllocationNode` that resides within
    /// the kernel heap region for its entire lifetime (`'static`).
    unsafe fn from_raw(ptr: *mut AllocNode) -> Self {
        // SAFETY: caller guarantees `ptr` is non-null.
        Self(unsafe { NonNull::new_unchecked(ptr) })
    }

    /// Recover the `NodePtr` for the header that precedes a user payload
    /// pointer returned by [`Allocator::alloc`].
    ///
    /// # Safety
    ///
    /// `user_ptr` must have been returned by a prior successful call to
    /// `Allocator::alloc`/`zalloc` and must not have been deallocated yet.
    unsafe fn from_user_ptr(user_ptr: *mut u8) -> Self {
        // SAFETY: the user pointer is `sizeof(AllocationNode)` bytes past the
        // header. Subtracting one `AllocationNode` recovers the header address.
        // The caller guarantees `user_ptr` originates from `alloc`, so this
        // pointer is valid, aligned, and inside the heap region.
        let header = unsafe { user_ptr.cast::<AllocNode>().offset(-1) };
        // SAFETY: `header` satisfies all `from_raw` preconditions per above.
        unsafe { Self::from_raw(header) }
    }

    /// Compute the pointer to the next node in the allocation list.
    ///
    /// Returns `None` if the next node would be at or past `tail`.
    fn next(self, tail: *const u8) -> Option<Self> {
        let size = self.as_ref().get_size();
        if size == 0 {
            return None;
        }
        // SAFETY: `self.0` points inside the heap region and `size` is the
        // total block size stored in the header. Adding `size` bytes yields
        // either the next valid header or the one-past-end sentinel (`tail`).
        let next_ptr = unsafe { self.0.as_ptr().cast::<u8>().add(size) };
        if next_ptr as *const u8 >= tail {
            return None;
        }
        // SAFETY: `next_ptr` is within the heap region (below `tail`) and
        // points to the start of the next `AllocationNode` header, which was
        // properly initialised when the region was split during allocation.
        Some(unsafe { Self::from_raw(next_ptr.cast::<AllocNode>()) })
    }

    /// Immutable reference to the underlying `AllocationNode`.
    fn as_ref(&self) -> &AllocNode {
        // SAFETY: the invariant on `NodePtr` guarantees the pointer is valid,
        // aligned, and the node is initialised for the `'static` lifetime.
        unsafe { self.0.as_ref() }
    }

    /// Mutable reference to the underlying `AllocationNode`.
    fn as_mut(&mut self) -> &mut AllocNode {
        // SAFETY: same as `as_ref`. Exclusive access is ensured by requiring
        // `&mut self` and the `SpinMutex` that guards the `Allocator`.
        unsafe { self.0.as_mut() }
    }

    /// Return the raw pointer.
    fn as_raw(self) -> *const AllocNode {
        self.0.as_ptr()
    }

    /// Return the user-facing payload pointer (one `AllocationNode` past the header).
    fn user_ptr(self) -> *mut u8 {
        // SAFETY: adding 1 to an `AllocationNode` pointer yields the payload
        // start, which is within the same allocation (header + payload).
        unsafe { self.0.as_ptr().add(1).cast() }
    }
}

/// Metadata for a region of byte-level allocation.
#[derive(Default)]
pub(crate) struct AllocNode(usize);

impl AllocNode {
    /// Flag the current node as being taken.
    pub(crate) const TAKEN_FLAG_MASK: usize = 1 << 63;

    /// Clear the taken flag.
    pub(crate) fn free(&mut self) {
        self.0 &= !Self::TAKEN_FLAG_MASK;
    }

    /// Return true if the node is free.
    pub(crate) fn is_free(&self) -> bool {
        self.0 & Self::TAKEN_FLAG_MASK == 0
    }

    /// Set the taken flag.
    pub(crate) fn take(&mut self) {
        self.0 |= Self::TAKEN_FLAG_MASK;
    }

    /// Return true if the node is taken.
    pub(crate) fn is_taken(&self) -> bool {
        self.0 & Self::TAKEN_FLAG_MASK == Self::TAKEN_FLAG_MASK
    }

    /// Set the node size.
    pub(crate) fn set_size(&mut self, size: usize) {
        let is_taken = self.is_taken();
        self.0 = size & !Self::TAKEN_FLAG_MASK;
        if is_taken {
            self.0 |= Self::TAKEN_FLAG_MASK;
        }
    }

    /// Get the node size.
    pub(crate) fn get_size(&self) -> usize {
        self.0 & !Self::TAKEN_FLAG_MASK
    }
}
