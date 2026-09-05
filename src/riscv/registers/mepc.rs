use core::arch::asm;

pub unsafe fn write(epc: usize) {
    unsafe {
        asm!("csrw mepc, {}", in(reg) epc);
    }
}
