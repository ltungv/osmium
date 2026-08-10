pub mod page;
pub mod ppn;
pub mod vpn;

mod buddy;
mod heap;

use core::alloc::{GlobalAlloc, Layout};

use crate::{
    HEAP_SIZE, HEAP_START,
    addr::PhysAddr,
    mem::{buddy::BuddyAlloc, page::MappedPageTable},
};

static BUDDY_ALLOC: spin::Once<spin::Mutex<BuddyAlloc>> = spin::Once::new();

static KHEAP: spin::Once<spin::Mutex<heap::Heap>> = spin::Once::new();

static PAGE_TABLE: spin::Once<spin::Mutex<MappedPageTable<'static>>> = spin::Once::new();

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

pub fn frame_allocator() -> &'static spin::Mutex<BuddyAlloc> {
    BUDDY_ALLOC.call_once(|| unsafe {
        buddy::BuddyAlloc::new(PhysAddr::new_trunc(HEAP_START), HEAP_SIZE)
            .map(spin::Mutex::new)
            .expect("`HEAP_START` and `HEAP_SIZE` represents a valid memory region")
    })
}

pub fn kheap() -> &'static spin::Mutex<heap::Heap> {
    KHEAP.call_once(|| {
        let mut allocator = frame_allocator().lock();
        heap::Heap::new(&mut allocator)
            .map(spin::Mutex::new)
            .expect("device has enough memory to accomodate the kernel's heep")
    })
}

pub fn page_table() -> &'static spin::Mutex<page::MappedPageTable<'static>> {
    PAGE_TABLE.call_once(|| {
        let mut allocator = frame_allocator().lock();
        page::MappedPageTable::new(&mut allocator)
            .map(spin::Mutex::new)
            .expect("kernel memory is identity mapped with the MMU")
    })
}

/// The global allocator is a static constant to a global allocator
/// structure. We don't need any members because we're using this
/// structure just to implement alloc and dealloc.
struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        kheap()
            .lock()
            .zalloc(layout.size())
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        kheap().lock().dealloc(ptr);
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
