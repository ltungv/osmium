//! Utilities for dealing with the memory system.

pub mod kalloc;
pub mod kheap;
pub mod paddr;
pub mod ppn;
pub mod slab;
pub mod vaddr;
pub mod vpn;

use crate::kern::mm::vaddr::VirtAddr;

/// The size of a page in bytes.
pub const PAGE_SIZE: usize = 4096;

/// Address of the trampoline in virtual memory spaces.
pub const TRAMPOLINE: usize = (1 << (VirtAddr::BITS - 1)) - PAGE_SIZE;

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

/// Address of the physical memory.
#[inline(always)]
pub fn mem_addr() -> usize {
    unsafe extern "C" {
        static _mem_addr: u8;
    }
    unsafe { &_mem_addr as *const u8 as usize }
}

/// Size of the physical memory.
#[inline(always)]
pub fn mem_size() -> usize {
    unsafe extern "C" {
        static _mem_size: u8;
    }
    unsafe { &_mem_size as *const u8 as usize }
}

/// Address of the `.tramp` section.
#[inline(always)]
pub fn tramp_addr() -> usize {
    unsafe extern "C" {
        static _tramp_addr: u8;
    }
    unsafe { &_tramp_addr as *const u8 as usize }
}

/// Address of the `.rodata` section.
#[inline(always)]
pub fn rodata_addr() -> usize {
    unsafe extern "C" {
        static _rodata_addr: u8;
    }
    unsafe { &_rodata_addr as *const u8 as usize }
}

/// Address of the `.data` section.
#[inline(always)]
pub fn data_addr() -> usize {
    unsafe extern "C" {
        static _data_addr: u8;
    }
    unsafe { &_data_addr as *const u8 as usize }
}

/// Address of the `.bss` section.
#[inline(always)]
pub fn bss_addr() -> usize {
    unsafe extern "C" {
        static _bss_addr: u8;
    }
    unsafe { &_bss_addr as *const u8 as usize }
}

/// Address of the kernel's stack.
#[inline(always)]
pub fn stack_addr() -> usize {
    unsafe extern "C" {
        static _stack_addr: u8;
    }
    unsafe { &_stack_addr as *const u8 as usize }
}

/// Address of the kernel's heap.
#[inline(always)]
pub fn heap_addr() -> usize {
    unsafe extern "C" {
        static _heap_addr: u8;
    }
    unsafe { &_heap_addr as *const u8 as usize }
}
