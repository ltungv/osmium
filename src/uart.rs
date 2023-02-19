//! This module contains the driver for the NS16550d UART hardware.

use core::{fmt::Write, hint::spin_loop};

use spin::mutex::{SpinMutex, SpinMutexGuard};

/// Print out using the global UART driver.
#[macro_export]
macro_rules! print {
    ($($args:tt)+) => ({
        use core::fmt::Write;
        let mut driver = $crate::uart::driver();
        let _ = write!(driver, $($args)+);
    });
}

/// Print out with a new line using the global UART driver.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\r\n")));
    ($fmt:expr, $($args:tt)+) => ($crate::print!(concat!($fmt, "\r\n"), $($args)+));
}

/// Default UART base address on the `virt` machine in QEMU.
pub const UART_BASE_ADDRESS: usize = 0x1000_0000;

/// The global uart driver instance.
static UART_DRIVER: SpinMutex<UartDriver> = SpinMutex::new(UartDriver(UART_BASE_ADDRESS));

/// Acquire unique access to the global UART driver.
pub fn driver() -> SpinMutexGuard<'static, UartDriver> {
    UART_DRIVER.lock()
}

/// Initialize the global UART driver state.
pub fn initialize() {
    UART_DRIVER.lock().initialize();
}

/// A driver for PC16550D (Universal Asynchronous Receiver/Transmitter With FIFOs).
#[derive(Debug)]
pub struct UartDriver(usize);

impl UartDriver {
    /// Put a byte into the Transmitter Holding Register (THR) blocking until the byte
    /// is ready to be sent.
    pub fn put(&self, byte: u8) -> Option<()> {
        unsafe {
            if self.lsr().read_volatile() & (1 << 6) == 0 {
                None
            } else {
                self.thr().write_volatile(byte);
                Some(())
            }
        }
    }

    /// Get the next available byte from the Receiver Buffer Register (RBR).
    pub fn get(&self) -> Option<u8> {
        unsafe {
            if self.lsr().read_volatile() & (1 << 0) == 0 {
                None
            } else {
                Some(self.rbr().read_volatile())
            }
        }
    }

    /// Initialize the UART hardware registers.
    fn initialize(&self) {
        // We'll later restore LCR to this value after setting the divisor.
        let lcr_value = 1 << 1 | 1 << 0;

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

        unsafe {
            // Enable FIFO, clear TX/RX queues, and set interrupt watermark at 14 bytes.
            self.fcr()
                .write_volatile(1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);
            // Set data word length to 8 bits.
            self.lcr().write_volatile(lcr_value);
            // Enable receiver buffer interrupts.
            self.ier().write_volatile(1 << 0);
            // Enable DLAB.
            self.lcr().write_volatile(lcr_value | 1 << 7);
            // Set divisor least significant bits.
            self.dll().write_volatile(divisor_ls as u8);
            // Set divisor most significant bits.
            self.dlm().write_volatile(divisor_ms as u8);
            // Disable DLAB.
            self.lcr().write_volatile(lcr_value);
            // Mark data terminal ready, and signal request to send.
            self.mcr().write_volatile(1 << 1 | 1 << 0);
        }
    }

    fn rbr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(0) }
    }

    fn thr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(0) }
    }

    fn dll(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(0) }
    }

    fn ier(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(1) }
    }

    fn dlm(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(1) }
    }

    fn fcr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(2) }
    }

    fn lcr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(3) }
    }

    fn mcr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(4) }
    }

    fn lsr(&self) -> *mut u8 {
        unsafe { (self.0 as *mut u8).add(5) }
    }
}

impl Write for UartDriver {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        s.bytes().for_each(|b| {
            while self.put(b).is_none() {
                spin_loop();
            }
        });
        Ok(())
    }
}
