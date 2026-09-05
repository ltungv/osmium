use core::arch::asm;

pub unsafe fn read() -> usize {
    let id: usize;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) id);
    }
    id
}
