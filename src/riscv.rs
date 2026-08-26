//! RISC-V hardware abstraction layer.
//!
//! This module provides safe and unsafe wrappers for interacting with RISC-V Control and Status Registers (CSRs)
//! and executing specific assembly instructions. These functions are essential for managing the CPU's state,
//! such as privilege levels, interrupts, exceptions, and paging.

use core::arch::asm;

/// Writes to the `mstatus` (Machine Status) register.
///
/// The `mstatus` register keeps track of and controls the hart's current operating state,
/// including enabling/disabling interrupts and setting the previous privilege mode.
/// This is typically used during boot to configure the CPU to enter supervisor mode.
pub unsafe fn w_mstatus(mstatus: usize) {
    unsafe {
        asm!("csrw mstatus, {}", in(reg) mstatus);
    }
}

/// Reads the `mstatus` (Machine Status) register.
///
/// See [`w_mstatus`] for more details on the register's purpose.
pub unsafe fn r_mstatus() -> usize {
    let mstatus;
    unsafe {
        asm!("csrr {}, mstatus", out(reg) mstatus);
    }
    mstatus
}

/// Writes to the `mepc` (Machine Exception Program Counter) register.
///
/// The `mepc` register holds the instruction address that the CPU will jump to
/// when executing an `mret` (machine return) instruction. This is used during kernel
/// initialization to transition to the main kernel entry point.
pub unsafe fn w_mepc(mepc: usize) {
    unsafe {
        asm!("csrw mepc, {}", in(reg) mepc);
    }
}

/// Writes to the `satp` (Supervisor Address Translation and Protection) register.
///
/// The `satp` register controls the supervisor-mode address translation and protection.
/// It configures the paging scheme (e.g., Sv39) and holds the physical page number (PPN)
/// of the root page table. Writing to this register is required to enable or disable paging.
pub unsafe fn w_satp(satp: usize) {
    unsafe {
        asm!("csrw satp, {}", in(reg) satp);
    }
}

/// Writes to the `medeleg` (Machine Exception Delegation) register.
///
/// This register indicates which exceptions should be delegated directly to supervisor mode,
/// bypassing machine mode. The OS uses this to handle faults (e.g., page faults) natively.
pub unsafe fn w_medeleg(medeleg: usize) {
    unsafe {
        asm!("csrw medeleg, {}", in(reg) medeleg);
    }
}

/// Writes to the `mideleg` (Machine Interrupt Delegation) register.
///
/// Similar to [`w_medeleg`], this register delegates specific interrupts (like timer or
/// external interrupts) to supervisor mode, allowing the kernel to handle them without
/// machine mode intervention.
pub unsafe fn w_mideleg(mideleg: usize) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) mideleg);
    }
}

/// Writes to the `sie` (Supervisor Interrupt Enable) register.
///
/// This register is used to enable or disable specific interrupts (like timer or
/// software interrupts) when executing in supervisor mode.
pub unsafe fn w_sie(sie: usize) {
    unsafe {
        asm!("csrw sie, {}", in(reg) sie);
    }
}

/// Reads the `sie` (Supervisor Interrupt Enable) register.
///
/// See [`w_sie`] for more details.
pub unsafe fn r_sie() -> usize {
    let sie;
    unsafe {
        asm!("csrr {}, sie", out(reg) sie);
    }
    sie
}

/// Writes to the `pmpaddr0` (Physical Memory Protection Address 0) register.
///
/// PMP registers are used in machine mode to grant supervisor mode access to specific
/// physical memory regions. This configures the address bound.
pub unsafe fn w_pmpaddr0(pmpaddr: usize) {
    unsafe {
        asm!("csrw pmpaddr0, {}", in(reg) pmpaddr);
    }
}

/// Writes to the `pmpcfg0` (Physical Memory Protection Configuration 0) register.
///
/// This configures the permissions (Read/Write/Execute) and addressing mode for the
/// physical memory region specified by [`w_pmpaddr0`].
pub unsafe fn w_pmpcfg0(pmpcfg: usize) {
    unsafe {
        asm!("csrw pmpcfg0, {}", in(reg) pmpcfg);
    }
}

/// Writes to the `menvcfg` (Machine Environment Configuration) register.
///
/// This register controls certain hardware behaviors, such as enabling hardware
/// updates of the Accessed (A) and Dirty (D) bits in page table entries.
pub unsafe fn w_menvcfg(menvcfg: usize) {
    unsafe {
        asm!("csrw menvcfg, {}", in(reg) menvcfg);
    }
}

/// Reads the `menvcfg` (Machine Environment Configuration) register.
///
/// See [`w_menvcfg`] for more details.
pub unsafe fn r_menvcfg() -> usize {
    let menvcfg;
    unsafe {
        asm!("csrr {}, menvcfg", out(reg) menvcfg);
    }
    menvcfg
}

/// Reads the `mhartid` (Machine Hardware Thread ID) register.
///
/// This register contains the integer ID of the hardware thread (hart) currently
/// executing the code. In a multicore system, this is used to identify the CPU.
pub unsafe fn r_mhartid() -> usize {
    let mhartid;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) mhartid);
    }
    mhartid
}

/// Writes to the `tp` (Thread Pointer) register.
///
/// In this kernel, the thread pointer is typically used to store the hart ID
/// of the current CPU, allowing fast access to CPU-specific local storage or identification.
pub unsafe fn w_tp(tp: usize) {
    unsafe {
        asm!("mv tp, {}", in(reg) tp);
    }
}

/// Reads the `tp` (Thread Pointer) register.
///
/// See [`w_tp`] for more details.
pub unsafe fn r_tp() -> usize {
    let tp;
    unsafe {
        asm!("mv {}, tp", out(reg) tp);
    }
    tp
}
