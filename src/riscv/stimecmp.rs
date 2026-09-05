use core::arch::asm;

pub unsafe fn write(time: usize) {
    unsafe {
        asm!("csrw stimecmp, {}", in(reg) time);
    }
}
