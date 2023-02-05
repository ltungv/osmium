//! This module contains the driver for the NS16550d UART hardware.

use core::{
    fmt::Write,
    hint::spin_loop,
    sync::atomic::{self, AtomicPtr},
};

use spin::mutex::SpinMutex;

/// Default UART base address on the `virt` machine in QEMU.
pub const QEMU_VIRT_UART_MMIO_ADDRESS: usize = 0x1000_0000;

/// The global uart driver instance.
pub static UART_DRIVER: spin::Once<SpinMutex<UartDriver>> = spin::Once::new();

/// Print out using the global UART driver.
#[macro_export]
macro_rules! print {
    ($($args:tt)+) => (
        if let Some(driver) = crate::uart_ns16550::UART_DRIVER.get() {
            use core::fmt::Write;
            let mut driver = driver.lock();
            let _ = write!(driver, $($args)+);
        }
    );
}

/// Print out with a new line using the global UART driver.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\r\n")));
    ($fmt:expr, $($args:tt)+) => ($crate::print!(concat!($fmt, "\r\n"), $($args)+));
}

/// A driver for PC16550D (Universal Asynchronous Receiver/Transmitter With FIFOs).
#[derive(Debug)]
pub struct UartDriver {
    rbr: AtomicPtr<u8>,
    thr: AtomicPtr<u8>,
    ier: AtomicPtr<u8>,
    fcr: AtomicPtr<u8>,
    lcr: AtomicPtr<u8>,
    mcr: AtomicPtr<u8>,
    lsr: AtomicPtr<u8>,
    dll: AtomicPtr<u8>,
    dlm: AtomicPtr<u8>,
}

impl Write for UartDriver {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        s.bytes().for_each(|b| unsafe { self.put(b) });
        Ok(())
    }
}

impl UartDriver {
    /// Create a new UART driver for the hardware at `base_address` in memory.
    pub unsafe fn new(base_address: usize) -> Self {
        let base_ptr = base_address as *mut u8;
        Self {
            rbr: AtomicPtr::new(base_ptr.add(0)),
            thr: AtomicPtr::new(base_ptr.add(0)),
            ier: AtomicPtr::new(base_ptr.add(1)),
            fcr: AtomicPtr::new(base_ptr.add(2)),
            lcr: AtomicPtr::new(base_ptr.add(3)),
            mcr: AtomicPtr::new(base_ptr.add(4)),
            lsr: AtomicPtr::new(base_ptr.add(5)),
            dll: AtomicPtr::new(base_ptr.add(0)),
            dlm: AtomicPtr::new(base_ptr.add(1)),
        }
    }

    /// Create a global UART driver and initialize it with reasonable configurations.
    pub unsafe fn initialize_global(base_address: usize) {
        UART_DRIVER.call_once(|| {
            let driver = Self::new(base_address);
            driver.initialize();
            SpinMutex::new(driver)
        });
    }

    /// Initialize the UART hardware registers.
    unsafe fn initialize(&self) {
        let ier = self.ier.load(atomic::Ordering::Relaxed);
        let fcr = self.fcr.load(atomic::Ordering::Relaxed);
        let lcr = self.lcr.load(atomic::Ordering::Relaxed);
        let mcr = self.mcr.load(atomic::Ordering::Relaxed);
        let dll = self.dll.load(atomic::Ordering::Relaxed);
        let dlm = self.dlm.load(atomic::Ordering::Relaxed);

        // We'll later restore LCR to this value after setting the divisor.
        let lcr_value = 1 << 1 | 1 << 0;

        // Enable FIFO, clear TX/RX queues, and set interrupt watermark at 14 bytes.
        fcr.write_volatile(1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);
        // Set data word length to 8 bits.
        lcr.write_volatile(lcr_value);
        // Enable receiver buffer interrupts.
        ier.write_volatile(1 << 0);

        // Set the divisor from a global clock rate of 22.729 MHz (22,729,000 cycles per second) to a signaling rate
        // of 2400 (BAUD). The formula given in the NS16500A specification for calculating the divisor is:
        // divisor = ceil((clock_hz) / (baud_sps x 16))
        // divisor = ceil(22_729_000 / (2400 x 16))
        // divisor = ceil(22_729_000 / 38_400)
        // divisor = ceil(591.901)
        // divisor = 592
        let divisor = 592u16;
        let divisor_ls = divisor & 0xff;
        let divisor_ms = divisor >> 8;

        // Enable DLAB.
        lcr.write_volatile(lcr_value | 1 << 7);
        // Set divisor least significant bits.
        dll.write_volatile(divisor_ls as u8);
        // Set divisor most significant bits.
        dlm.write_volatile(divisor_ms as u8);
        // Disable DLAB.
        lcr.write_volatile(lcr_value);

        // Mark data terminal ready, and signal request to send.
        mcr.write_volatile(1 << 1 | 1 << 0);
    }

    /// Put a byte into the Transmitter Holding Register (THR) blocking until the byte
    /// is ready to be sent.
    pub unsafe fn put(&self, byte: u8) {
        let thr = self.thr.load(atomic::Ordering::Relaxed);
        let lsr = self.lsr.load(atomic::Ordering::Relaxed);
        while lsr.read_volatile() & (1 << 6) == 0 {
            spin_loop();
        }
        thr.write_volatile(byte);
    }

    /// Get the next available byte from the Receiver Buffer Register (RBR).
    pub unsafe fn get(&self) -> Option<u8> {
        let rbr = self.rbr.load(atomic::Ordering::Relaxed);
        let lsr = self.lsr.load(atomic::Ordering::Relaxed);
        if lsr.read_volatile() & (1 << 0) == 0 {
            None
        } else {
            Some(rbr.read_volatile())
        }
    }
}
