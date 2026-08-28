use core::panic::PanicInfo;

use crate::{abort, print, println};

pub const SIFIVE_BASE: usize = 0x10_0000;

pub trait Case {
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

pub fn run(cases: &[&dyn Case]) {
    println!("Running {} tests", cases.len());
    for case in cases {
        case.test();
    }
    exit_qemu(SiFiveTestStatus::Success);
}

pub fn panic(info: &PanicInfo) -> ! {
    println!("[failed]\n");
    println!("Error: {}\n", info);
    exit_qemu(SiFiveTestStatus::Failure(1))
}

#[derive(Clone, Copy)]
#[repr(u16)]
pub enum SiFiveTestStatus {
    Failure(u16),
    Success,
}

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
