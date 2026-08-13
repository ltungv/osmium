pub mod page;
pub mod ppn;
pub mod vpn;

mod frame;
mod heap;

use core::alloc::Layout;

use crate::{
    BSS_END, BSS_START, DATA_END, DATA_START, HEAP_SIZE, HEAP_START, KERNEL_STACK_END,
    KERNEL_STACK_START, PAGE_SIZE, RODATA_END, RODATA_START, TEXT_END, TEXT_START,
    addr::{PhysAddr, VirtAddr, phys_to_virt},
    mem::{
        frame::BuddyAlloc,
        heap::LockedLinkedHeap,
        page::{MappedPageTable, PteFlags},
    },
    uart,
};

static FRAME_ALLOC: spin::Mutex<BuddyAlloc> = spin::Mutex::new(BuddyAlloc::new());

static PAGE_TABLE: spin::Mutex<MappedPageTable<'static>> = spin::Mutex::new(MappedPageTable::new());

#[global_allocator]
static KHEAP: LockedLinkedHeap = LockedLinkedHeap::new();

pub fn init_frame_allocator() {
    let mut allocator = FRAME_ALLOC.lock();
    unsafe {
        allocator.init(PhysAddr::new_trunc(HEAP_START), HEAP_SIZE);
    }
}

pub fn init_page_table() {
    let (kheap_start, kheap_end) = { (KHEAP.start_addr(), KHEAP.end_addr()) };
    let mut allocator = FRAME_ALLOC.lock();
    let mut page_table = PAGE_TABLE.lock();
    unsafe {
        page_table
            .init(&mut allocator)
            .expect("kernel's page table should have been initialized");
    }
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
            .expect("`HEAP` memory section should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(TEXT_START),
                VirtAddr::new_trunc(TEXT_END),
                PteFlags::R | PteFlags::X,
                &mut allocator,
            )
            .expect("`TEXT` memory section should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(RODATA_START),
                VirtAddr::new_trunc(RODATA_END),
                PteFlags::R | PteFlags::X,
                &mut allocator,
            )
            .expect("`RODATA` memory section should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(DATA_START),
                VirtAddr::new_trunc(DATA_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`DATA` memory section should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(BSS_START),
                VirtAddr::new_trunc(BSS_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`BSS` memory section should be mapped");

        page_table
            .map_range(
                VirtAddr::new_trunc(KERNEL_STACK_START),
                VirtAddr::new_trunc(KERNEL_STACK_END),
                PteFlags::R | PteFlags::W,
                &mut allocator,
            )
            .expect("`KERNEL_STACK` memory section should be mapped");
    }

    page_table
        .map_range(
            phys_to_virt(uart::QEMU_ADDR),
            phys_to_virt(uart::QEMU_ADDR) + 256,
            PteFlags::R | PteFlags::W,
            &mut allocator,
        )
        .expect("16550 UART device addresses should be mapped");

    page_table
        .map_range(
            VirtAddr::new_trunc(kheap_start),
            VirtAddr::new_trunc(kheap_end),
            PteFlags::R | PteFlags::W,
            &mut allocator,
        )
        .expect("`KERNEL_HEAP` memory section should be mapped");
}

pub fn init_kheap() {
    let mut allocator = FRAME_ALLOC.lock();
    let ppn = allocator
        .alloc(6)
        .expect("device should have enough memory for the kernel's heap");

    let heap_start = unsafe { phys_to_virt(ppn.addr()).as_ptr_mut::<u8>() as usize };
    let heap_size = PAGE_SIZE * (1 << 6);
    KHEAP.init(heap_start, heap_size);
}

pub fn frame_allocator() -> &'static spin::Mutex<BuddyAlloc> {
    &FRAME_ALLOC
}

pub fn page_table() -> &'static spin::Mutex<page::MappedPageTable<'static>> {
    &PAGE_TABLE
}

pub fn kheap() -> &'static LockedLinkedHeap {
    &KHEAP
}

#[alloc_error_handler]
fn global_alloc_error(l: Layout) -> ! {
    panic!(
        "failed to allocate {} bytes with {}-byte alignment.",
        l.size(),
        l.align()
    );
}
