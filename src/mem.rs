pub mod ppn;
pub mod vpn;

mod buddy;
mod heap;
mod page;

use core::alloc::{GlobalAlloc, Layout};

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
    addr::PhysAddr,
    mem::{
        buddy::BuddyAlloc,
        page::{MappedPageTable, PteFlags},
    },
    uart,
};

static BUDDY_ALLOC: spin::Once<BuddyAlloc> = spin::Once::new();

static KHEAP: spin::Once<spin::Mutex<heap::Heap>> = spin::Once::new();

static PAGE_TABLE: spin::Once<spin::Mutex<MappedPageTable<'static>>> = spin::Once::new();

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

/// Initialize the global frame allocator.
pub fn initialize_frame_allocator() {
    BUDDY_ALLOC.call_once(|| unsafe {
        buddy::BuddyAlloc::new(PhysAddr::new_trunc(HEAP_START), HEAP_SIZE)
            .expect("`HEAP_START` and `HEAP_SIZE` represents a valid memory region")
    });
}

/// Initialize the kernel heap allocator.
pub fn initialize_kheap_allocator() {
    KHEAP.call_once(|| {
        heap::Heap::new(buddy())
            .map(spin::Mutex::new)
            .expect("device has enough memory to accomodate the kernel's heep")
    });
}

/// Initialize the kernel page table.
pub fn initialize_page_table() {
    fn init() -> Result<MappedPageTable<'static>, page::Error> {
        let mut page_table = page::MappedPageTable::new(buddy())?;
        page_table.map_range(
            PhysAddr::new_trunc(uart::QEMU_ADDR),
            PhysAddr::new_trunc(uart::QEMU_ADDR) + 256,
            PteFlags::R | PteFlags::W,
            buddy(),
        )?;

        let (kheap_start, kheap_end) = {
            let heap = kheap();
            (heap.start(), heap.end())
        };
        page_table.map_range(kheap_start, kheap_end, PteFlags::R | PteFlags::W, buddy())?;

        // SAFETY: the linker-script symbols below are valid addresses
        // provided by the linker and represent the kernel's memory layout.
        unsafe {
            page_table.map_range(
                PhysAddr::new_trunc(HEAP_START),
                PhysAddr::new_trunc(HEAP_START) + HEAP_SIZE,
                PteFlags::R | PteFlags::W,
                buddy(),
            )?;

            page_table.map_range(
                PhysAddr::new_trunc(TEXT_START),
                PhysAddr::new_trunc(TEXT_END),
                PteFlags::R | PteFlags::X,
                buddy(),
            )?;

            page_table.map_range(
                PhysAddr::new_trunc(RODATA_START),
                PhysAddr::new_trunc(RODATA_END),
                PteFlags::R | PteFlags::X,
                buddy(),
            )?;

            page_table.map_range(
                PhysAddr::new_trunc(DATA_START),
                PhysAddr::new_trunc(DATA_END),
                PteFlags::R | PteFlags::W,
                buddy(),
            )?;

            page_table.map_range(
                PhysAddr::new_trunc(BSS_START),
                PhysAddr::new_trunc(BSS_END),
                PteFlags::R | PteFlags::W,
                buddy(),
            )?;

            page_table.map_range(
                PhysAddr::new_trunc(KERNEL_STACK_START),
                PhysAddr::new_trunc(KERNEL_STACK_END),
                PteFlags::R | PteFlags::W,
                buddy(),
            )?;
        }
        Ok(page_table)
    }
    PAGE_TABLE.call_once(|| {
        init()
            .map(spin::Mutex::new)
            .expect("kernel memory is identity mapped with the MMU")
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

// TODO: Expose `alloc/dealloc` so user can't take a `MutexGuard` and accidentally deadlock.
/// Get a reference to the kernel heap allocator.
pub fn page_table() -> spin::MutexGuard<'static, page::MappedPageTable<'static>, spin::Spin> {
    PAGE_TABLE
        .get()
        .expect("kernel's page table has been initialized")
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
