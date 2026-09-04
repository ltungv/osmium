//! Helpers for working with RISC-V assembly and registers.

#![expect(missing_docs, clippy::missing_safety_doc)]

use core::arch::asm;

use crate::mem::ppn::PhysPageNumber;

pub unsafe fn wfi() {
    unsafe {
        asm!("wfi");
    }
}

pub unsafe fn mret() {
    unsafe {
        asm!("mret");
    }
}

pub unsafe fn sfence_vma() {
    unsafe {
        asm!("sfence.vma zero, zero");
    }
}

pub unsafe fn w_pmp0(addr: usize, cfg: PmpCfg) {
    unsafe {
        asm!("csrw pmpaddr0, {}", in(reg) addr);
        asm!("csrw pmpcfg0, {}", in(reg) cfg.bits);
    }
}

pub unsafe fn r_tp() -> usize {
    let id: usize;
    unsafe {
        asm!("mv {}, tp", out(reg) id);
    }
    id
}

pub unsafe fn w_tp(id: usize) {
    unsafe {
        asm!("mv tp, {}", in(reg) id);
    }
}

pub unsafe fn r_mhartid() -> usize {
    let id: usize;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) id);
    }
    id
}

pub unsafe fn r_mcounteren() -> Mcounteren {
    let bits: u32;
    unsafe {
        asm!("csrr {}, mcounteren", out(reg) bits);
    }
    Mcounteren { bits }
}

pub unsafe fn w_mcounteren(mcounteren: Mcounteren) {
    unsafe {
        asm!("csrw mcounteren, {}", in(reg) mcounteren.bits);
    }
}

pub unsafe fn r_menvcfg() -> Menvcfg {
    let bits: usize;
    unsafe {
        asm!("csrr {}, menvcfg", out(reg) bits);
    }
    Menvcfg { bits }
}

pub unsafe fn w_menvcfg(menvcfg: Menvcfg) {
    unsafe {
        asm!("csrw menvcfg, {}", in(reg) menvcfg.bits);
    }
}

pub unsafe fn r_mstatus() -> Mstatus {
    let bits: usize;
    unsafe {
        asm!("csrr {}, mstatus", out(reg) bits);
    }
    Mstatus { bits }
}

pub unsafe fn w_mstatus(mstatus: Mstatus) {
    unsafe {
        asm!("csrw mstatus, {}", in(reg) mstatus.bits);
    }
}

pub unsafe fn w_mepc(mepc: usize) {
    unsafe {
        asm!("csrw mepc, {}", in(reg) mepc);
    }
}

pub unsafe fn w_medeleg(flags: ExceptionFlags) {
    unsafe {
        asm!("csrw medeleg, {}", in(reg) flags.bits());
    }
}

pub unsafe fn w_mideleg(flags: InterruptFlags) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) flags.bits());
    }
}

pub unsafe fn w_satp(satp: Satp) {
    unsafe {
        asm!("csrw satp, {}", in(reg) satp.bits);
    }
}

pub unsafe fn r_sstatus() -> Sstatus {
    let bits: usize;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) bits);
    }
    Sstatus { bits }
}

pub unsafe fn rc_sstatus(mut bits: usize) -> Sstatus {
    unsafe {
        asm!("csrrc {}, sstatus, {}", out(reg) bits, in(reg) bits);
    }
    Sstatus { bits }
}

pub unsafe fn s_sstatus(bits: usize) {
    unsafe {
        asm!("csrs sstatus, {}", in(reg) bits);
    }
}

pub unsafe fn w_sstatus(sstatus: Sstatus) {
    unsafe {
        asm!("csrw sstatus, {}", in(reg) sstatus.bits);
    }
}

pub unsafe fn r_sepc() -> usize {
    let sepc: usize;
    unsafe {
        asm!("csrr {}, sepc", out(reg) sepc);
    }
    sepc
}

pub unsafe fn w_sepc(sepc: usize) {
    unsafe {
        asm!("csrw sepc, {}", in(reg) sepc);
    }
}

pub unsafe fn w_stvec(stvec: usize) {
    unsafe {
        asm!("csrw stvec, {}", in(reg) stvec);
    }
}

pub unsafe fn r_scause() -> TrapCause {
    let scause: usize;
    unsafe {
        asm!("csrr {}, scause", out(reg) scause);
    }
    TrapCause::from_code(scause).expect("scause should contain only known bits")
}

pub unsafe fn r_sie() -> InterruptFlags {
    let bits: usize;
    unsafe {
        asm!("csrr {}, sie", out(reg) bits);
    }
    InterruptFlags::from_bits_retain(bits)
}

pub unsafe fn w_sie(flags: InterruptFlags) {
    unsafe {
        asm!("csrw sie, {}", in(reg) flags.bits());
    }
}

pub unsafe fn r_time() -> usize {
    let time: usize;
    unsafe {
        asm!("csrr {}, time", out(reg)time);
    }
    time
}

pub unsafe fn w_stimecmp(time: usize) {
    unsafe {
        asm!("csrw stimecmp, {}", in(reg) time);
    }
}

/// RISC-V privilege levels.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Privilege {
    /// The privilege level intended to be used by an application.
    ///
    /// Many RISC-V implementations will also support at least user mode (U-mode) to protect the
    /// rest of the system from application code.
    ///
    /// Applications run on an application execution environment (AEE), and are coded to run
    /// with a particular application binary interface (ABI). The ABI includes the supported
    /// user-level ISA plus a set of ABI calls to interact with the AEE. The ABI hides details
    /// of the AEE from the application to allow greater flexibility in implementing the AEE.
    User = 0b00,

    /// The privilege level intended to be used by an operating system.
    ///
    /// Conventional operating system (OS) can support multiprogrammed execution of multiple
    /// applications. Each application communicates over an ABI with the OS, which provides
    /// the AEE. Operating systems themselves interface with a supervisor execution environment
    /// (SEE) via a supervisor binary interface (SBI). An SBI comprises the user-level and
    /// supervisor-level ISA together with a set of SBI function calls. Using a single SBI
    /// across all SEE implementations allows a single OS binary image to run on any SEE.
    ///
    /// The SEE can be a simple boot loader and BIOS-style IO system in a low-end hardware
    /// platform, or a hypervisor-provided virtual machine in a high-end server, or a thin
    /// translation layer over a host operating system in an architecture simulation
    /// environment.
    Supervisor = 0b01,

    /// The highest privilege level and is the only mandatory privilege leve for a RISC-V
    /// hardware platform. Code in machine mode is usually inherently trusted, since it has
    /// low-level access to the machine implementation.
    Machine = 0b11,
}

/// Causes for RISC-V's exceptions.
#[derive(Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ExceptionCause {
    InstructionAddressMisaligned = 0x0000,
    InstructionAccessFault = 0x0001,
    IllegalInstruction = 0x0002,
    Breakpoint = 0x0003,
    LoadAddressMisaligned = 0x0004,
    LoadAddressFault = 0x0005,
    StoreAddressMisaligned = 0x0006,
    StoreAddressFault = 0x0007,
    EnvironmentCallFromUMode = 0x0008,
    EnvironmentCallFromSMode = 0x0009,
    EnvironmentCallFromMMode = 0x000b,
    InstructionPageFault = 0x000c,
    LoadPageFault = 0x000d,
    StorePageFault = 0x000f,
}

impl ExceptionCause {
    pub const fn from_code(code: usize) -> Option<Self> {
        let cause = match code {
            0x0000 => Self::InstructionAddressMisaligned,
            0x0001 => Self::InstructionAccessFault,
            0x0002 => Self::IllegalInstruction,
            0x0003 => Self::Breakpoint,
            0x0004 => Self::LoadAddressMisaligned,
            0x0005 => Self::LoadAddressFault,
            0x0006 => Self::StoreAddressMisaligned,
            0x0007 => Self::StoreAddressFault,
            0x0008 => Self::EnvironmentCallFromUMode,
            0x0009 => Self::EnvironmentCallFromSMode,
            0x000b => Self::EnvironmentCallFromMMode,
            0x000c => Self::InstructionPageFault,
            0x000d => Self::LoadPageFault,
            0x000f => Self::StorePageFault,
            _ => return None,
        };
        Some(cause)
    }
}

bitflags::bitflags! {
    pub struct ExceptionFlags: usize {
        const INSTRUCTION_ADDRESS_MISALIGNED = 1 << ExceptionCause::InstructionAddressMisaligned as usize;
        const INSTRUCTION_ACCESS_FAULT = 1 << ExceptionCause::InstructionAccessFault as usize;
        const ILLEGAL_INSTRUCTION = 1 << ExceptionCause::IllegalInstruction as usize;
        const BREAKPOINT = 1 << ExceptionCause::Breakpoint as usize;
        const LOAD_ADDRESS_MISALIGNED = 1 << ExceptionCause::LoadAddressMisaligned as usize;
        const LOAD_ADDRESS_FAULT = 1 << ExceptionCause::LoadAddressFault as usize;
        const STORE_ADDRESS_MISALIGNED = 1 << ExceptionCause::StoreAddressMisaligned as usize;
        const STORE_ADDRESS_FAULT = 1 << ExceptionCause::StoreAddressFault as usize;
        const ENVIRONMENT_CALL_FROM_UMODE = 1 << ExceptionCause::EnvironmentCallFromUMode as usize;
        const ENVIRONMENT_CALL_FROM_SMODE = 1 << ExceptionCause::EnvironmentCallFromSMode as usize;
        const ENVIRONMENT_CALL_FROM_MMODE = 1 << ExceptionCause::EnvironmentCallFromMMode as usize;
        const INSTRUCTION_PAGE_FAULT = 1 << ExceptionCause::InstructionPageFault as usize;
        const LOAD_PAGE_FAULT = 1 << ExceptionCause::LoadPageFault as usize;
        const STORE_PAGE_FAULT = 1 << ExceptionCause::StorePageFault as usize;
    }
}

/// Causes for RISC-V's interrupts.
#[derive(Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum InterruptCause {
    SupervisorSoftware = 0x0001,
    MachineSoftware = 0x0003,
    SupervisorTimer = 0x0005,
    MachineTimer = 0x0007,
    SupervisorExternal = 0x0009,
    MachineExternal = 0x000b,
    CounterOverflow = 0x000d,
}

impl InterruptCause {
    pub const fn from_code(cause: usize) -> Option<Self> {
        let cause = match cause {
            0x0001 => Self::SupervisorSoftware,
            0x0003 => Self::MachineSoftware,
            0x0005 => Self::SupervisorTimer,
            0x0007 => Self::MachineTimer,
            0x0009 => Self::SupervisorExternal,
            0x000b => Self::MachineExternal,
            0x000d => Self::CounterOverflow,
            _ => return None,
        };
        Some(cause)
    }
}

bitflags::bitflags! {
    pub struct InterruptFlags: usize {
        const SUPERVISOR_SOFTWARE = 1 << InterruptCause::SupervisorSoftware as usize;
        const MACHINE_SOFTWARE = 1 << InterruptCause::MachineSoftware as usize;
        const SUPERVISOR_TIMER = 1 << InterruptCause::SupervisorTimer as usize;
        const MACHINE_TIMER = 1 << InterruptCause::MachineTimer as usize;
        const SUPERVISOR_EXTERNAL = 1 << InterruptCause::SupervisorExternal as usize;
        const MACHINE_EXTERNAL = 1 << InterruptCause::MachineExternal as usize;
        const COUNTER_OVERFLOW = 1 << InterruptCause::CounterOverflow as usize;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrapCause {
    Exception(ExceptionCause),
    Interrupt(InterruptCause),
}

impl TrapCause {
    pub const fn from_code(cause: usize) -> Option<Self> {
        let intr_mask = 1 << (usize::BITS - 1);
        let code_mask = intr_mask - 1;
        let code = cause & code_mask;
        if cause & intr_mask == intr_mask {
            let Some(cause) = InterruptCause::from_code(code) else {
                return None;
            };
            Some(Self::Interrupt(cause))
        } else {
            let Some(cause) = ExceptionCause::from_code(code) else {
                return None;
            };
            Some(Self::Exception(cause))
        }
    }
}

pub struct PmpCfg {
    bits: u8,
}

impl PmpCfg {
    const R: u8 = 1 << 0;
    const W: u8 = 1 << 1;
    const X: u8 = 1 << 2;

    pub const fn napot() -> Self {
        Self { bits: 0b11 << 3 }
    }

    pub const fn readable(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::R;
        } else {
            self.bits &= !Self::R;
        }
        self
    }

    pub const fn writeable(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::W;
        } else {
            self.bits &= !Self::W;
        }
        self
    }

    pub const fn executable(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::X;
        } else {
            self.bits &= !Self::X;
        }
        self
    }
}

pub struct Mcounteren {
    bits: u32,
}

impl Mcounteren {
    const TM: u32 = 1 << 1;

    pub const fn tm(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::TM;
        } else {
            self.bits &= !Self::TM;
        }
        self
    }
}

pub struct Menvcfg {
    bits: usize,
}

impl Menvcfg {
    const ADUE: usize = 1 << 61;
    const STCE: usize = 1 << 63;

    pub const fn adue(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::ADUE;
        } else {
            self.bits &= !Self::ADUE;
        }
        self
    }

    pub const fn stce(mut self, bit: bool) -> Self {
        if bit {
            self.bits |= Self::STCE;
        } else {
            self.bits &= !Self::STCE;
        }
        self
    }
}

pub struct Mstatus {
    bits: usize,
}

impl Mstatus {
    pub const fn mpp(mut self, mpp: Privilege) -> Self {
        self.bits &= !(0b11 << 11);
        self.bits |= (mpp as usize) << 11;
        self
    }
}

pub struct Satp {
    bits: usize,
}

impl Satp {
    pub const fn bare() -> Self {
        Self { bits: 0 }
    }

    pub const fn sv39(ppn: PhysPageNumber) -> Self {
        Self {
            bits: 0b0100 << 60 | ppn.get(),
        }
    }
}

pub struct Sstatus {
    bits: usize,
}

impl Sstatus {
    pub const SIE: usize = 1 << 1;

    pub const fn has(&self, bits: usize) -> bool {
        self.bits & bits == bits
    }

    pub const fn get_spp(&self) -> Privilege {
        let mask = 1 << 8;
        if self.bits & mask == mask {
            Privilege::Supervisor
        } else {
            Privilege::User
        }
    }
}
