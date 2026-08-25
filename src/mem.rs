//! Utilities for dealing with the memory system.

pub mod paddr;
pub mod ppn;
pub mod vaddr;
pub mod vpn;

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
