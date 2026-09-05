use core::arch::asm;

pub unsafe fn read() -> usize {
    let epc: usize;
    unsafe {
        asm!("csrr {}, sepc", out(reg) epc);
    }
    epc
}

pub unsafe fn write(epc: usize) {
    unsafe {
        asm!("csrw sepc, {}", in(reg) epc);
    }
}
