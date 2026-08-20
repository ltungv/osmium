use crate::riscv::r_tp;

pub fn cpuid() -> usize {
    unsafe { r_tp() }
}
