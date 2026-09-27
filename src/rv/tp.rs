use core::arch::asm;

pub unsafe fn read() -> usize {
    let tp: usize;
    unsafe {
        asm!("mv {}, tp", out(reg) tp);
    }
    tp
}

pub unsafe fn write(tp: usize) {
    unsafe {
        asm!("mv tp, {}", in(reg) tp);
    }
}
