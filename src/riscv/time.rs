use core::arch::asm;

pub unsafe fn read() -> usize {
    let time: usize;
    unsafe {
        asm!("csrr {}, time", out(reg)time);
    }
    time
}
