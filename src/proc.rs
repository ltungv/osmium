//! Process management and CPU identification.

use core::arch::asm;

/// Get the hardware thread id of the currently executing thread.
///
/// # Safety
///
/// Interrupt must be disable before calling this function to avoid racing with another thread on
/// the `tp` register when a context switch occurs.
pub unsafe fn cpuid() -> usize {
    let tp: usize;
    unsafe {
        asm!("mv {}, tp", out(reg) tp);
    }
    tp
}
