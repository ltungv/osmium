pub mod ppn;
pub mod vpn;

mod buddy;
mod heap;
mod page;

use core::alloc::{GlobalAlloc, Layout};

use crate::{HEAP_SIZE, HEAP_START, mem::buddy::BuddyAlloc};

static BUDDY_ALLOC: spin::Once<BuddyAlloc> = spin::Once::new();

static KHEAP: spin::Once<spin::Mutex<heap::Heap>> = spin::Once::new();

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

/// Initialize the global frame allocator.
pub fn initialize_frame_allocator() {
    BUDDY_ALLOC.call_once(|| unsafe {
        buddy::BuddyAlloc::new(HEAP_START.into(), HEAP_SIZE)
            .expect("`HEAP_START` and `HEAP_SIZE` represents a valid memory region")
    });
}

/// Initialize the kernel heap allocator.
pub fn initialize_kheap_allocator() {
    KHEAP.call_once(|| {
        let allocator = heap::Heap::new(buddy())
            .expect("device has enough memory to accomodate the kernel's heep");

        allocator
            .identity_map()
            .expect("kernel's address space is identity mapped");

        spin::Mutex::new(allocator)
    });
}

/// Get a reference to the frame allocator.
pub fn buddy() -> &'static BuddyAlloc {
    BUDDY_ALLOC
        .get()
        .expect("kernel's frame allocator has been initialized")
}

// TODO: Expose `alloc/dealloc` so user can't take a `MutexGuard` and accidentally deadlock.
/// Get a reference to the kernel heap allocator.
pub fn kheap() -> spin::MutexGuard<'static, heap::Heap, spin::Spin> {
    KHEAP
        .get()
        .expect("kernel's heap allocator has been initialized")
        .lock()
}

/// The global allocator is a static constant to a global allocator
/// structure. We don't need any members because we're using this
/// structure just to implement alloc and dealloc.
struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        kheap()
            .zalloc(layout.size())
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        kheap().dealloc(ptr);
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
