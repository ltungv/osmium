mod buddy_alloc;
// pub mod frame;
pub mod heap;
pub mod page;
pub mod ppn;
pub mod vpn;

use core::alloc::{GlobalAlloc, Layout};

use crate::{HEAP_SIZE, HEAP_START, mem::buddy_alloc::BuddyAllocator};

static FRAME_ALLOCATOR: spin::Once<spin::Mutex<BuddyAllocator>> = spin::Once::new();

static KHEAP_ALLOCATOR: spin::Once<spin::Mutex<heap::Allocator>> = spin::Once::new();

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

/// Initialize the global frame allocator.
pub fn initialize_frame_allocator() {
    FRAME_ALLOCATOR.call_once(|| unsafe {
        spin::Mutex::new(buddy_alloc::BuddyAllocator::new(
            HEAP_START.into(),
            HEAP_SIZE,
        ))
    });
}

/// Initialize the kernel heap allocator.
pub fn initialize_kheap_allocator() {
    KHEAP_ALLOCATOR.call_once(|| {
        let allocator = heap::Allocator::new(&mut frame_allocator())
            .expect("initialized kernel heap allocator");

        allocator.identity_map().expect("mapped kernel memory");
        spin::Mutex::new(allocator)
    });
}

// TODO: Expose `alloc/dealloc` so user can't take a `MutexGuard` and accidentally deadlock.
/// Get a reference to the frame allocator.
pub fn frame_allocator() -> spin::MutexGuard<'static, BuddyAllocator, spin::Spin> {
    FRAME_ALLOCATOR
        .get()
        .expect("initialized frame allocator")
        .lock()
}

// TODO: Expose `alloc/dealloc` so user can't take a `MutexGuard` and accidentally deadlock.
/// Get a reference to the kernel heap allocator.
pub fn kheap_allocator() -> spin::MutexGuard<'static, heap::Allocator, spin::Spin> {
    KHEAP_ALLOCATOR
        .get()
        .expect("initialized kernel heap allocator")
        .lock()
}

/// The global allocator is a static constant to a global allocator
/// structure. We don't need any members because we're using this
/// structure just to implement alloc and dealloc.
struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        kheap_allocator()
            .zalloc(layout.size())
            .unwrap_or(core::ptr::null_mut())
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
