use core::panic::PanicInfo;

use crate::{print, println};

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

pub fn runner(cases: &[&dyn Case]) {
    println!("Running {} tests", cases.len());
    for case in cases {
        case.test();
    }
}

pub fn panic_handler(info: &PanicInfo) -> ! {
    println!("[failed]\n");
    println!("Error: {}\n", info);
    loop {
        core::hint::spin_loop();
    }
}

fn exit_qemu() {
    todo!()
}
