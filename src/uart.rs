//! Driver for the NS16550D UART hardware.

use core::{fmt::Write, hint::spin_loop};

use spin::{
    Once,
    mutex::{SpinMutex, SpinMutexGuard},
};

/// Print to a monitor using UART.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        use core::fmt::Write;
        let mut driver = $crate::uart::driver();
        let _ = write!(driver, $($args)+);
    }};
}

/// Print to a monitor using UART, with a newline.
#[macro_export]
macro_rules! println {
    () => (print!("\r\n"));
    ($($arg:tt)*) => (print!("{}\r\n", format_args!($($arg)*)));
}

/// Default UART base address on the `virt` machine in QEMU.
pub(crate) const BASE_ADDRESS: usize = 0x1000_0000;

/// Global UART driver instance.
static DRIVER: Once<SpinMutex<UartDriver>> = Once::new();

/// Initialize the global UART driver state.
pub(crate) fn initialize() {
    DRIVER.call_once(|| {
        let mut driver = unsafe { UartDriver::new(BASE_ADDRESS) };
        driver.initialize();
        SpinMutex::new(driver)
    });
}

/// Acquire unique access to the global UART driver.
pub(crate) fn driver() -> SpinMutexGuard<'static, UartDriver> {
    DRIVER.get().expect("initialized UART driver").lock()
}

/// A driver for NS16550D (Universal Asynchronous Receiver/Transmitter with FIFOs).
#[derive(Debug)]
pub(crate) struct UartDriver(&'static mut [u8; 8]);

impl UartDriver {
    /// Receiver holding register.
    const RHR: usize = 0b000;

    /// Transmitter holding register.
    const THR: usize = 0b000;

    /// Interrupt enable register
    const IER: usize = 0b001;

    /// Interrupt status register
    const ISR: usize = 0b010;

    /// FIFO control register
    const FCR: usize = 0b010;

    /// Line control register
    const LCR: usize = 0b011;

    /// Modem control register
    const MCR: usize = 0b100;

    /// Line status register
    const LSR: usize = 0b101;

    /// Modem status register
    const MSR: usize = 0b110;

    /// Scratch pad register
    const SPR: usize = 0b111;

    /// Divisor latch, least significant byte
    const DLL: usize = 0b000;

    /// Divisor latch, most significant byte
    const DLM: usize = 0b001;

    /// Prescaler division
    const PSD: usize = 0b101;

    /// Create a new UART driver with the given base address.
    ///
    /// # Safety
    ///
    /// The given address must be the memory-mapped physical address of the UART hardware.
    pub(crate) const unsafe fn new(addr: usize) -> Self {
        let ptr = addr as *mut [u8; 8];
        Self(unsafe { &mut *ptr })
    }

    /// Put a byte into the transmitter holding register (thr) blocking until the byte is ready to be sent.
    pub(crate) fn put(&mut self, byte: u8) -> Option<()> {
        if self.rd_reg(Self::LSR) & (1 << 6) == 0 {
            None
        } else {
            self.wr_reg(Self::THR, byte);
            Some(())
        }
    }

    /// Get the next available byte from the receiver buffer register (rbr).
    pub(crate) fn get(&self) -> Option<u8> {
        if self.rd_reg(Self::LSR) & (1 << 0) == 0 {
            None
        } else {
            Some(self.rd_reg(Self::RHR))
        }
    }

    /// Read a byte from a register offset
    fn rd_reg(&self, offset: usize) -> u8 {
        unsafe { core::ptr::read_volatile(&self.0[offset]) }
    }

    /// Write a byte to a register offset
    fn wr_reg(&mut self, offset: usize, value: u8) {
        unsafe { core::ptr::write_volatile(&mut self.0[offset], value) };
    }

    /// Initialize the UART hardware registers.
    fn initialize(&mut self) {
        // We'll later restore lcr to this value after setting the divisor.
        let lcr_value = 1 << 1 | 1 << 0;

        // Set the divisor from a global clock rate of 22.729 mhz (22,729,000 cycles per second)
        // to a signaling rate of 2400 (baud). The formula given in the ns16500a specification
        // for calculating the divisor is:
        // divisor = ceil((clock_hz) / (baud_sps x 16))
        // divisor = ceil(22_729_000 / (2400 x 16))
        // divisor = ceil(22_729_000 / 38_400)
        // divisor = ceil(591.901)
        // divisor = 592
        let divisor = 592u16;
        let divisor_ls = divisor & 0xff;
        let divisor_ms = divisor >> 8;

        // Enable fifo, clear tx/rx queues, and set interrupt watermark at 14 bytes.
        self.wr_reg(Self::FCR, 1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);
        // Set data word length to 8 bits.
        self.wr_reg(Self::LCR, lcr_value);
        // Enable receiver buffer interrupts.
        self.wr_reg(Self::IER, 1 << 0);
        // Enable dlab.
        self.wr_reg(Self::LCR, lcr_value | 1 << 7);
        // Set divisor least significant bits.
        self.wr_reg(Self::DLL, divisor_ls as u8);
        // Set divisor most significant bits.
        self.wr_reg(Self::DLM, divisor_ms as u8);
        // Disable dlab.
        self.wr_reg(Self::LCR, lcr_value);
        // Mark data terminal ready, and signal request to send.
        self.wr_reg(Self::MCR, 1 << 1 | 1 << 0);
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
