//! A RISC-V kernel.

#![feature(alloc_error_handler)]
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

pub mod driver;
pub mod mm;
pub mod rt;
