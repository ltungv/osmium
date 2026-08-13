pub mod page;
pub mod ppn;
pub mod vpn;

mod frame;
mod heap;

use core::alloc::{GlobalAlloc, Layout};

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, PAGE_SIZE, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
    addr::{PhysAddr, VirtAddr, phys_to_virt},
    mem::{
        frame::BuddyAlloc,
        heap::LinkedHeap,
        page::{MappedPageTable, PteFlags},
    },
    uart,
};

static FRAME_ALLOC: spin::Mutex<BuddyAlloc> = spin::Mutex::new(BuddyAlloc::new());

static KHEAP: spin::Mutex<LinkedHeap> = spin::Mutex::new(LinkedHeap::new());

static PAGE_TABLE: spin::Mutex<MappedPageTable<'static>> = spin::Mutex::new(MappedPageTable::new());

pub fn frame_allocator() -> &'static spin::Mutex<BuddyAlloc> {
    &FRAME_ALLOC
}

pub fn kheap() -> &'static spin::Mutex<LinkedHeap> {
    &KHEAP
}

pub fn page_table() -> &'static spin::Mutex<MappedPageTable<'static>> {
    &PAGE_TABLE
}

pub fn init_frame_allocator() {
    let mut allocator = FRAME_ALLOC.lock();
    unsafe {
        allocator.init(PhysAddr::new_trunc(HEAP_START), HEAP_SIZE);
    }
}

pub fn init_kheap() {
    let mut allocator = FRAME_ALLOC.lock();
    let ppn = allocator
        .alloc(6)
        .expect("device should have memory for the kernel's heap");

    unsafe {
        KHEAP
            .lock()
            .init(phys_to_virt(ppn.addr()), PAGE_SIZE * (1 << 6));
    }
}

pub fn init_page_table() {
    let (kheap_start, kheap_end) = {
        let kheap = KHEAP.lock();
        (kheap.start_addr(), kheap.end_addr())
    };

    let mut allocator = FRAME_ALLOC.lock();
    let ppn = allocator
        .alloc(0)
        .expect("device should have memory for the root page table");

    let mut page_table = PAGE_TABLE.lock();
    unsafe { page_table.init(ppn) }

    // SAFETY: the linker-script symbols below are valid addresses
    // provided by the linker and represent the kernel's memory layout.
    unsafe {
        page_table
            .map_range(
                VirtAddr::new_trunc(HEAP_START),
                VirtAddr::new_trunc(HEAP_START) + HEAP_SIZE,
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`HEAP` memory region should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(TEXT_START),
                VirtAddr::new_trunc(TEXT_END),
                PteFlags::R | PteFlags::X,
                &mut allocator,
            )
            .expect("`TEXT` memory region should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(RODATA_START),
                VirtAddr::new_trunc(RODATA_END),
                PteFlags::R | PteFlags::X,
                &mut allocator,
            )
            .expect("`RODATA` memory region should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(DATA_START),
                VirtAddr::new_trunc(DATA_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`DATA` memory region should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(BSS_START),
                VirtAddr::new_trunc(BSS_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`BSS` memory region should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(KERNEL_STACK_START),
                VirtAddr::new_trunc(KERNEL_STACK_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`KERNEL_STACK` memory region should be mapped");
    }

    page_table
        .map_range(
            phys_to_virt(uart::QEMU_ADDR),
            phys_to_virt(uart::QEMU_ADDR) + 256,
            PteFlags::R | PteFlags::W,
            &mut allocator,
        )
        .expect("16550 UART device memory region should be mapped");

    page_table
        .map_range(
            VirtAddr::new_trunc(kheap_start),
            VirtAddr::new_trunc(kheap_end),
            PteFlags::R | PteFlags::W,
            &mut allocator,
        )
        .expect("kernel's memory region should be mapped");
}

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { KHEAP.lock().alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { KHEAP.lock().dealloc(ptr, layout) }
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
