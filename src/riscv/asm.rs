//! RISC-V assembly instructions.

use core::arch::asm;

/// SFENCE.VMA instruction wrapper (all address spaces and page table levels).
#[inline(always)]
pub fn sfence_vma_all() {
    unsafe {
        asm!("sfence.vma zero, zero");
    }
}

/// WFI instruction wrapper.
#[inline(always)]
pub fn wfi() {
    unsafe {
        asm!("wfi");
    }
}
