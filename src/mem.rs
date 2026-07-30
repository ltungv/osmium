pub(crate) mod addr;
pub(crate) mod frame;
pub(crate) mod heap;
pub(crate) mod page;
pub(crate) mod ppn;
pub(crate) mod vpn;

use core::alloc::{GlobalAlloc, Layout};

use spin::{
    Once,
    mutex::{SpinMutex, SpinMutexGuard},
};

use crate::{HEAP_SIZE, HEAP_START};

/// The size of a page in bytes.
const PAGE_SIZE: usize = 1 << PAGE_SIZE_BITS;

/// The number of bits needed to represent the page size.
const PAGE_SIZE_BITS: usize = 12;

static FRAME_ALLOCATOR: Once<frame::Allocator> = Once::new();

static KHEAP_ALLOCATOR: Once<SpinMutex<heap::Allocator>> = Once::new();

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

/// Initialize the global frame allocator.
pub(crate) fn initialize_frame_allocator() {
    FRAME_ALLOCATOR.call_once(|| unsafe { frame::Allocator::new(HEAP_START, HEAP_SIZE) });
}

/// Initialize the kernel heap allocator.
pub(crate) fn initialize_kheap_allocator() {
    KHEAP_ALLOCATOR.call_once(|| {
        let mut allocator =
            heap::Allocator::new(frame_allocator()).expect("initialized kernel heap allocator");

        allocator.identity_map().expect("mapped kernel memory");
        SpinMutex::new(allocator)
    });
}

/// Get a reference to the frame allocator.
pub(crate) fn frame_allocator() -> &'static frame::Allocator {
    FRAME_ALLOCATOR.get().expect("initialized frame allocator")
}

/// Get a reference to the kernel heap allocator.
pub(crate) fn kheap_allocator() -> SpinMutexGuard<'static, heap::Allocator> {
    KHEAP_ALLOCATOR
        .get()
        .expect("initialized kernel heap allocator")
        .lock()
}

// The global allocator is a static constant to a global allocator
// structure. We don't need any members because we're using this
// structure just to implement alloc and dealloc.
struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        kheap_allocator()
            .zalloc(layout.size())
            .expect("allocated layout")
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        kheap_allocator().dealloc(ptr);
    }
}

#[alloc_error_handler]
fn global_alloc_error(l: Layout) -> ! {
    panic!(
        "failed to allocate {} bytes with {}-byte alignment.",
        l.size(),
        l.align()
    );
}
