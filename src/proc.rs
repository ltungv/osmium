use crate::riscv::r_tp;

/// Get the hardware thread id of the currently executing thread.
///
/// # Safety
///
/// Interrupt must be disable before calling this function to avoid racing with another thread on
/// the `tp` register when a context switch occurs.
pub unsafe fn cpuid() -> usize {
    unsafe { r_tp() }
}
