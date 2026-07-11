//! driver for the ns16550d uart hardware.

use core::{fmt::Write, hint::spin_loop};

use spin::mutex::{SpinMutex, SpinMutexGuard};

/// print to a monitor using uart.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        use core::fmt::Write;
        let mut driver = $crate::driver::uart::driver();
        let _ = write!(driver, $($args)+);

    }};
}

/// print to a monitor using uart, with a newline.
#[macro_export]
macro_rules! println {
    () => (print!("\r\n"));
    ($($arg:tt)*) => (print!("{}\r\n", format_args!($($arg)*)));
}

/// default uart base address on the `virt` machine in qemu.
const UART_BASE_ADDRESS: usize = 0x1000_0000;

/// global uart driver instance.
static UART_DRIVER: SpinMutex<UartDriver> = SpinMutex::new(UartDriver(UART_BASE_ADDRESS));

/// acquire unique access to the global uart driver.
pub fn driver() -> SpinMutexGuard<'static, UartDriver> {
    UART_DRIVER.lock()
}

/// initialize the global uart driver state.
pub fn initialize() {
    UART_DRIVER.lock().initialize();
}

/// a driver for pc16550d (universal asynchronous receiver/transmitter with fifos).
#[derive(Debug)]
pub struct UartDriver(usize);

impl UartDriver {
    /// put a byte into the transmitter holding register (thr)
    /// blocking until the byte is ready to be sent.
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

    /// get the next available byte from the receiver buffer register (rbr).
    pub fn get(&self) -> Option<u8> {
        unsafe {
            if self.lsr().read_volatile() & (1 << 0) == 0 {
                None
            } else {
                Some(self.rbr().read_volatile())
            }
        }
    }

    /// initialize the uart hardware registers.
    fn initialize(&self) {
        // we'll later restore lcr to this value after setting the divisor.
        let lcr_value = 1 << 1 | 1 << 0;

        // set the divisor from a global clock rate of 22.729 mhz (22,729,000 cycles per second)
        // to a signaling rate of 2400 (baud). the formula given in the ns16500a specification
        // for calculating the divisor is:
        // divisor = ceil((clock_hz) / (baud_sps x 16))
        // divisor = ceil(22_729_000 / (2400 x 16))
        // divisor = ceil(22_729_000 / 38_400)
        // divisor = ceil(591.901)
        // divisor = 592
        let divisor = 592u16;
        let divisor_ls = divisor & 0xff;
        let divisor_ms = divisor >> 8;

        unsafe {
            // enable fifo, clear tx/rx queues, and set interrupt watermark at 14 bytes.
            self.fcr()
                .write_volatile(1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);
            // set data word length to 8 bits.
            self.lcr().write_volatile(lcr_value);
            // enable receiver buffer interrupts.
            self.ier().write_volatile(1 << 0);
            // enable dlab.
            self.lcr().write_volatile(lcr_value | 1 << 7);
            // set divisor least significant bits.
            self.dll().write_volatile(divisor_ls as u8);
            // set divisor most significant bits.
            self.dlm().write_volatile(divisor_ms as u8);
            // disable dlab.
            self.lcr().write_volatile(lcr_value);
            // mark data terminal ready, and signal request to send.
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
