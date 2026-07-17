//! Memory management.

pub mod alloc;
pub mod frame;
pub mod page;

use core::{fmt, num::NonZero, ops::Add};

use crate::{
    driver::uart,
    mm::{self, alloc::KernelMemory, frame::FrameAllocator, page::Sv39PageTableEntryFlags},
};

use spin::{
    Once,
    mutex::{SpinMutex, SpinMutexGuard},
};

const PAGE_ORDER: usize = 12;

const PAGE_SIZE: usize = 1 << PAGE_ORDER;

unsafe extern "C" {
    /// First memory address in the `.text` section.
    pub static TEXT_START: usize;

    /// Last memory address in the `.text` section.
    pub static TEXT_END: usize;

    /// First memory address in the `.rodata` section.
    pub static RODATA_START: usize;

    /// Last memory address in the `.rodata` section.
    pub static RODATA_END: usize;

    /// First memory address in the `.data` section.
    pub static DATA_START: usize;

    /// Last memory address in the `.data` section.
    pub static DATA_END: usize;

    /// First memory address in the `.bss` section.
    pub static BSS_START: usize;

    /// Last memory address in the `.bss` section.
    pub static BSS_END: usize;

    /// First memory address of the kernel's stack.
    pub static KERNEL_STACK_START: usize;

    /// Last memory address of the kernel's stack.
    pub static KERNEL_STACK_END: usize;

    /// First memory address of the heap.
    pub static HEAP_START: usize;

    /// Size of the heap in bytes.
    pub static HEAP_SIZE: usize;

    /// First memory address.
    pub static MEMORY_START: usize;

    /// Last memory address.
    pub static MEMORY_END: usize;
}

static FRAME_ALLOCATOR: Once<FrameAllocator> = Once::new();

static OS_KMEM: Once<SpinMutex<KernelMemory>> = Once::new();

/// Initialize the memory management system.
pub fn initialize() {
    FRAME_ALLOCATOR.call_once(|| {
        let heap_start = unsafe { NonZero::new(mm::HEAP_START) };
        let heap_size = unsafe { NonZero::new(mm::HEAP_SIZE) };
        FrameAllocator::new(
            heap_start.expect("non-zero heap start"),
            heap_size.expect("non-zero heap size"),
        )
    });
    OS_KMEM.call_once(|| {
        SpinMutex::new(KernelMemory::new(mm::framer()).expect("Could not allocate kernel memory."))
    });
}

/// Grabs the physical frame allocator.
pub fn framer() -> &'static FrameAllocator {
    FRAME_ALLOCATOR.get().expect("frame allocator initialized")
}

/// Get a reference to the kernel memory.
pub fn kmem() -> SpinMutexGuard<'static, KernelMemory> {
    OS_KMEM.get().expect("frame allocator initialized").lock()
}

const fn align_value(val: usize, order: usize) -> usize {
    assert!(order > 0);
    let o = (1usize << order) - 1;
    (val + o) & !o
}

/// Identity map all sections of the kernel's memory.
pub fn id_map() -> Result<(), mm::page::TableError> {
    let kernel_memory = mm::kmem();
    let frame_allocator = mm::framer();

    let (kmem_start, kmem_end) = {
        let alloc_list = kernel_memory.allocation_list();
        (alloc_list.head(), alloc_list.tail())
    };
    let root = unsafe { &mut *kernel_memory.page_table_addr() };

    root.map(
        frame_allocator,
        uart::BASE_ADDRESS.into(),
        uart::BASE_ADDRESS.into(),
        Sv39PageTableEntryFlags::default()
            .set_readable(true)
            .set_writeable(true),
        0,
    )?;

    root.id_map_range(
        frame_allocator,
        kmem_start,
        kmem_end,
        Sv39PageTableEntryFlags::default()
            .set_readable(true)
            .set_writeable(true),
    )?;

    unsafe {
        root.id_map_range(
            frame_allocator,
            HEAP_START,
            HEAP_START + HEAP_SIZE / PAGE_SIZE,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_writeable(true),
        )?;

        root.id_map_range(
            frame_allocator,
            TEXT_START,
            TEXT_END,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_executable(true),
        )?;

        root.id_map_range(
            frame_allocator,
            RODATA_START,
            RODATA_END,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_executable(true),
        )?;

        root.id_map_range(
            frame_allocator,
            DATA_START,
            DATA_END,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_writeable(true),
        )?;

        root.id_map_range(
            frame_allocator,
            BSS_START,
            BSS_END,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_writeable(true),
        )?;

        root.id_map_range(
            frame_allocator,
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            Sv39PageTableEntryFlags::default()
                .set_readable(true)
                .set_writeable(true),
        )?;
    }
    Ok(())
}

/// A physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(usize);

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Phys({:x})", self.0)
    }
}

impl<T> From<*const T> for PhysAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for PhysAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl PhysAddr {
    /// The zero physical address.
    pub const ZERO: Self = Self(0);

    /// Decomposes the physical address into physical page numbers (PPNs).
    pub fn ppns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x3ff_ffff,
        ]
    }

    /// Offset in bytes between this physical address and another one.
    pub fn offset_from(self, other: Self) -> isize {
        self.0 as isize - other.0 as isize
    }

    /// Derives a raw pointer from this address.
    pub fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

/// A virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(usize);

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Virt({:x})", self.0)
    }
}

impl<T> From<*const T> for VirtAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl VirtAddr {
    /// The zero virtual address.
    pub const ZERO: Self = Self(0);

    /// Decompose the virtual address into virtual page numbers (VPNs).
    pub fn vpns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x1ff,
        ]
    }
}
