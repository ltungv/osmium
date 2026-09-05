//! Helpers for working with RISC-V assembly and registers.

pub mod asm;
pub mod registers;

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
    /// Causes for RISC-V's exceptions encoded as bit flags.
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
    /// Causes for RISC-V's interrupts encoded as bit flags.
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
