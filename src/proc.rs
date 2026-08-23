use crate::riscv::r_tp;

pub unsafe fn cpuid() -> usize {
    unsafe { r_tp() }
}
