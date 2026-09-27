//! A RISC-V kernel.

pub mod mm;
mod proc;
pub mod sync;
mod trap;
pub mod vm;

#[cfg(test)]
pub mod test;

use core::{
    arch::naked_asm,
    sync::atomic::{self, AtomicBool},
};

use crate::{
    dev::uart,
    kern::proc::CpuPin,
    println,
    rv::{
        self, ExceptionFlags, InterruptFlags, Privilege,
        mcounteren::{self, Mcounteren},
        medeleg,
        menvcfg::{self, Menvcfg},
        mepc, mhartid, mideleg, mstatus,
        pmp::{self, PmpCfg},
        satp::{self, Satp},
        sie, stimecmp, tp, wfi,
    },
};

use mm::{bss_addr, kalloc, kheap, stack_addr};

/// Kernel error.
#[derive(Debug)]
pub enum Error {
    /// The kernel could not map a virtual address to a physical address.
    BadMapping(vm::MappingError),

    /// There's no memory left on the device for the kernel.
    OutOfMemory,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMapping(err) => write!(f, "{err}"),
            Self::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

#[unsafe(link_section = ".text.init")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn _entry() {
    naked_asm!(
        "la sp, _stack_addr",
        "la a0, 0x4000",
        "csrr a1, mhartid",
        "addi a1, a1, 1",
        "mul a0, a0, a1",
        "add sp, sp, a0",
        "call minit",
        "mret",
    )
}

/// Create a function with the unmangled name "minit".
///
/// This function will be called by the entrypoint in `entry.S` in machine mode to configure the
/// system. Once configured, all CPUs switch into supervisor mode and jump the address of the
/// function that was given to this macro.
#[macro_export]
macro_rules! start {
    ($path:path) => {
        #[unsafe(export_name = "minit")]
        extern "C" fn __impl_start() {
            // validate the signature of the program entry point
            let _: extern "C" fn() = $path;
            // setup the system in machine mode before switching to supervisor mode and jumps to the
            // function located at `$path`
            $crate::kern::minit($path as *const () as usize);
        }
    };
}

/// Initialize the system in machine mode then switch into supervisor mode.
#[inline(always)]
pub fn minit(mepc: usize) {
    unsafe {
        // initialize the bss memory section to 0
        // only one cpu is responsible for writing, and there always exists a cpu with id 0
        let hartid = mhartid::read();
        if hartid == 0 {
            let ptr = bss_addr() as *mut u8;
            let len = stack_addr() - bss_addr();
            core::slice::from_raw_parts_mut(ptr, len).fill(0);
        }
        // set `mstatus.mpp` to 1, so the cpu switch into supervisor mode after `mret` is called
        mstatus::write(mstatus::read().with_mpp(Privilege::Supervisor));
        // set `mepc`, so the cpu jumps to the address in `mepc` after `mret` is called
        mepc::write(mepc);
        // set `satp` to disable paging
        satp::write(Satp::bare());
        // delegate all exceptions and interrupts to supervisor mode
        medeleg::write(ExceptionFlags::all());
        mideleg::write(InterruptFlags::all());
        // set `sie` to enable specific interrupts:
        sie::write(
            sie::read() | InterruptFlags::SUPERVISOR_TIMER | InterruptFlags::SUPERVISOR_EXTERNAL,
        );
        // give supervisor mode access to all physical memory
        pmp::write0(
            0x3f_ffff_ffff_ffff,
            PmpCfg::napot()
                .with(PmpCfg::R)
                .with(PmpCfg::W)
                .with(PmpCfg::X),
        );
        // enable hardware updates of page table entries' a and d bits
        menvcfg::write(menvcfg::read().with(Menvcfg::ADUE).with(Menvcfg::STCE));
        // allow supervisor to use stimecmp and time
        mcounteren::write(mcounteren::read().with(Mcounteren::TM));
        // ask for the first timer interrupt
        stimecmp::write(rv::time::read() + 1_000_000);
        // set the thread pointer to the current cpu id
        tp::write(hartid);
    }
}

/// Initialize the system in supervisor mode.
pub fn sinit() {
    static INIT: AtomicBool = AtomicBool::new(false);
    let pin = CpuPin::new();
    if pin.cpuid() == 0 {
        uart::init();
        println!();
        println!("osmium kernel is booting");
        println!();
        // page allocator
        kalloc::init();
        // kernel page table
        vm::kvminit();
        // enable paging
        vm::kvminithart();
        // object allocator
        kheap::init();
        // install trap vectors
        trap::inithart();
        // finish initialization
        INIT.store(true, atomic::Ordering::Release);
    } else {
        // wait for cpu 0 to finish initialization
        while !INIT.load(atomic::Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // enable paging
        vm::kvminithart();
        // install trap vectors
        trap::inithart();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("aborting!");
    if let Some(p) = info.location() {
        println!("panic: {} ({}:{})", info.message(), p.file(), p.line());
    } else {
        println!("panic: no information available");
    }
    abort()
}

/// Abort execution, preventing the current CPU from running.
fn abort() -> ! {
    loop {
        wfi();
    }
}
