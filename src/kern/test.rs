//! Custom test runner for the kernel.

use crate::{
    kern::{abort, proc::CpuPin},
    print, println,
};

/// A simple runner that sequentially goes over all test cases.
pub fn runner(cases: &[&dyn Case]) {
    let pin = CpuPin::new();
    if pin.cpuid() == 0 {
        println!("running {} tests", cases.len());
        for case in cases {
            case.test();
        }
        exit_qemu(SiFiveTestStatus::Success);
    } else {
        abort();
    }
}

/// Test's panic handler.
#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    println!("[failed]\n");
    println!("error: {}\n", info);
    exit_qemu(SiFiveTestStatus::Failure(1))
}

/// A trait for test cases.
pub trait Case {
    /// Run the test case.
    fn test(&self) -> ();
}

impl<T> Case for T
where
    T: Fn(),
{
    fn test(&self) {
        print!("{}...\t", core::any::type_name::<T>());
        self();
        println!("[ok]");
    }
}

/// Address of the SiFive test device.
pub const SIFIVE_BASE: usize = 0x10_0000;

/// Exit statuses for the SiFive test device.
#[derive(Clone, Copy)]
#[repr(u16)]
pub enum SiFiveTestStatus {
    /// Signal that the test failed with an exit code.
    Failure(u16),

    /// Signal that the test succeeded.
    Success,
}

/// Turn the system off by sending a status code to the SiFive test device.
pub fn exit_qemu(status: SiFiveTestStatus) -> ! {
    let command = match status {
        SiFiveTestStatus::Failure(code) => (code as u32) << 16 | 0x3333,
        SiFiveTestStatus::Success => 0x5555,
    };
    unsafe {
        core::ptr::write_volatile(SIFIVE_BASE as *mut u32, command);
    }
    abort()
}
