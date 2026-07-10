//! A RISCV kernel

#![no_std]
#![deny(missing_docs)]
#![warn(
    clippy::all,
    rustdoc::all,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![feature(panic_info_message, alloc_error_handler)]

pub mod driver;
pub mod mem;
pub mod runtime;

/// Aligns (set to a multiple of some power of two) and always rounds up.
/// This takes an order which is the exponent to 2^order, therefore,
/// all alignments must be made as a power of two.
const fn align_value(val: usize, order: usize) -> usize {
    let o = (1usize << order) - 1;
    (val + o) & !o
}
