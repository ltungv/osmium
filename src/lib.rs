//! a risc-v kernel.

#![no_std]
#![warn(
    clippy::all,
    rustdoc::all,
    missing_docs,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2021_compatibility,
    rust_2024_compatibility
)]

/// hardware driver.
pub mod driver;
/// memory management.
pub mod mem;
/// system runtime.
pub mod rt;
