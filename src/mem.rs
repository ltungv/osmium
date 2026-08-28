//! Utilities for dealing with the memory system.

pub mod paddr;
pub mod ppn;
pub mod vaddr;
pub mod vpn;

/// The size of a page in bytes.
pub const PAGE_SIZE: usize = 4096;

unsafe extern "C" {
    /// Address of the physical memory.
    pub static MEM_ADDR: usize;

    /// Size of the physical memory.
    pub static MEM_SIZE: usize;

    /// Address of the `.tramp` section.
    pub static TRAMP_ADDR: usize;

    /// Address of the `.rodata` section.
    pub static RODATA_ADDR: usize;

    /// Address of the `.data` section.
    pub static DATA_ADDR: usize;

    /// Address of the `.bss` section.
    pub static BSS_ADDR: usize;

    /// Address of the kernel's stack.
    pub static STACK_ADDR: usize;

    /// Address of the kernel's heap.
    pub static HEAP_ADDR: usize;
}

/// Align the value `x` downwards to a multiple of `align`.
pub const fn align_down(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    x & !(align - 1)
}

/// Align the value `x` upwards to a multiple of `align`.
pub const fn align_up(x: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of two");
    (x + align - 1) & !(align - 1)
}
