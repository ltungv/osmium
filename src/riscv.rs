use core::arch::asm;

pub unsafe fn w_mstatus(mstatus: usize) {
    unsafe {
        asm!("csrw mstatus, {}", in(reg) mstatus);
    }
}

pub unsafe fn r_mstatus() -> usize {
    let mstatus;
    unsafe {
        asm!("csrr {}, mstatus", out(reg) mstatus);
    }
    mstatus
}

pub unsafe fn w_mepc(mepc: usize) {
    unsafe {
        asm!("csrw mepc, {}", in(reg) mepc);
    }
}

pub unsafe fn w_satp(satp: usize) {
    unsafe {
        asm!("csrw satp, {}", in(reg) satp);
    }
}

pub unsafe fn w_medeleg(medeleg: usize) {
    unsafe {
        asm!("csrw medeleg, {}", in(reg) medeleg);
    }
}

pub unsafe fn w_mideleg(mideleg: usize) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) mideleg);
    }
}

pub unsafe fn w_sie(sie: usize) {
    unsafe {
        asm!("csrw sie, {}", in(reg) sie);
    }
}

pub unsafe fn r_sie() -> usize {
    let sie;
    unsafe {
        asm!("csrr {}, sie", out(reg) sie);
    }
    sie
}

pub unsafe fn w_pmpaddr0(pmpaddr: usize) {
    unsafe {
        asm!("csrw pmpaddr0, {}", in(reg) pmpaddr);
    }
}

pub unsafe fn w_pmpcfg0(pmpcfg: usize) {
    unsafe {
        asm!("csrw pmpcfg0, {}", in(reg) pmpcfg);
    }
}

pub unsafe fn w_menvcfg(menvcfg: usize) {
    unsafe {
        asm!("csrw menvcfg, {}", in(reg) menvcfg);
    }
}

pub unsafe fn r_menvcfg() -> usize {
    let menvcfg;
    unsafe {
        asm!("csrr {}, menvcfg", out(reg) menvcfg);
    }
    menvcfg
}

pub unsafe fn r_mhartid() -> usize {
    let mhartid;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) mhartid);
    }
    mhartid
}

pub unsafe fn w_tp(tp: usize) {
    unsafe {
        asm!("mv tp, {}", in(reg) tp);
    }
}

pub unsafe fn r_tp() -> usize {
    let tp;
    unsafe {
        asm!("mv {}, tp", out(reg) tp);
    }
    tp
}
