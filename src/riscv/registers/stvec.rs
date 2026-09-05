use core::arch::asm;

pub unsafe fn write(tvec: usize) {
    unsafe {
        asm!("csrw stvec, {}", in(reg) tvec);
    }
}
