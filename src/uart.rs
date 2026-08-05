//! A driver for 16550 UART devices, typically known and used as serial ports or COM ports.

use core::{
    fmt::{self, Write},
    num::NonZeroU8,
    ptr::NonNull,
};

/// Print to an UART port.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        use core::fmt::Write;
        let mut driver = $crate::uart::driver();
        let _ = write!(driver, $($args)+);
    }};
}

/// Print to an UART port, with a newline.
#[macro_export]
macro_rules! println {
    () => (print!("\r\n"));
    ($($arg:tt)*) => (print!("{}\r\n", format_args!($($arg)*)));
}

/// Default UART base address on the `virt` machine in QEMU.
pub const BASE_ADDRESS: usize = 0x1000_0000;

static UART16550: spin::Once<spin::Mutex<Uart16550>> = spin::Once::new();

/// Initialize the global UART driver state.
pub fn initialize() {
    UART16550.call_once(|| {
        let mut driver = unsafe {
            let base = NonNull::new_unchecked(BASE_ADDRESS as *mut u8);
            Uart16550::new(base, 1).expect("16550 UART device address is valid")
        };
        driver.initialize();
        spin::Mutex::new(driver)
    });
}

/// Acquire unique access to the global UART driver.
pub fn driver() -> spin::MutexGuard<'static, Uart16550, spin::Spin> {
    UART16550
        .get()
        .expect("16550 UART device driver is initialized")
        .lock()
}

#[derive(Debug)]
pub enum InvalidAddressError {
    /// The given base pointer is invalid, e.g., it can't accomodate [`NUM_REGISTERS`]
    /// consecutive addresses.
    InvalidBase(NonNull<u8>),

    /// The given stride is invalid. A stride must be non-zero and a power of two.
    InvalidStride(u8),
}

impl fmt::Display for InvalidAddressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidBase(base) => {
                write!(f, "{base:p} is not a valid 16550 UART device base address")
            }
            Self::InvalidStride(stride) => write!(
                f,
                "stride must be non-zero and a power of two; got {stride}"
            ),
        }
    }
}

/// A driver for 16550 UART devices backed by memory-mapped I/O addresses.
pub struct Uart16550 {
    base: NonNull<u8>,
    stride: NonZeroU8,
}

// SAFETY: `Uart16550` is not `Sync`, so concurrent access from multiple thread is not possible
// without additional synchronizations. The base pointer is ensured to points to a physical memory
// region large enough to accomodate `NUM_REGISTERS` addresses, and access to the region is given
// exclusively to the driver instance. All operations take a `&mut self` which ensures the driver is
// only accessed by at most one thread at a time.
unsafe impl Send for Uart16550 {}

#[allow(dead_code)]
impl Uart16550 {
    /// Receiver holding register.
    const RHR: usize = 0;

    /// Transmitter holding register.
    const THR: usize = 0;

    /// Interrupt enable register
    const IER: usize = 1;

    /// Interrupt status register
    const ISR: usize = 2;

    /// FIFO control register
    const FCR: usize = 2;

    /// Line control register
    const LCR: usize = 3;

    /// Modem control register
    const MCR: usize = 4;

    /// Line status register
    const LSR: usize = 5;

    /// Modem status register
    const MSR: usize = 6;

    /// Scratch pad register
    const SPR: usize = 7;

    /// Divisor latch, least significant byte
    const DLL: usize = 0;

    /// Divisor latch, most significant byte
    const DLM: usize = 1;

    /// Prescaler division
    const PSD: usize = 3;

    /// Number of registers of the device.
    const NUM_REGISTERS: usize = 8;

    pub unsafe fn new(base: NonNull<u8>, stride: u8) -> Result<Self, InvalidAddressError> {
        if !stride.is_power_of_two() {
            return Err(InvalidAddressError::InvalidStride(stride));
        }
        let Some(stride) = NonZeroU8::new(stride) else {
            return Err(InvalidAddressError::InvalidStride(stride));
        };
        if (base.as_ptr() as usize)
            .checked_add((Self::NUM_REGISTERS - 1) * stride.get() as usize)
            .is_none()
        {
            return Err(InvalidAddressError::InvalidBase(base));
        }
        Ok(Self { base, stride })
    }

    /// Put a byte into the transmitter holding register (thr) blocking until the byte is ready to be sent.
    pub(crate) fn put(&mut self, byte: u8) -> bool {
        if self.rd_reg(Self::LSR) & (1 << 6) == 0 {
            false
        } else {
            self.wr_reg(Self::THR, byte);
            true
        }
    }

    /// Get the next available byte from the receiver buffer register (rbr).
    pub(crate) fn get(&mut self) -> Option<u8> {
        if self.rd_reg(Self::LSR) & (1 << 0) == 0 {
            None
        } else {
            Some(self.rd_reg(Self::RHR))
        }
    }

    /// Read a byte from a register offset
    fn rd_reg(&mut self, offset: usize) -> u8 {
        unsafe {
            self.base
                .add(offset * self.stride.get() as usize)
                .read_volatile()
        }
    }

    /// Write a byte to a register offset
    fn wr_reg(&mut self, offset: usize, value: u8) {
        unsafe {
            self.base
                .add(offset * self.stride.get() as usize)
                .write_volatile(value);
        }
    }

    /// Initialize the UART hardware registers.
    fn initialize(&mut self) {
        // Disable all interrupts during initialization.
        self.wr_reg(Self::IER, 0);

        // Data word length: 8 bits.
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
        {
            // Enable DLAB to access the divisor latches (offsets 0 and 1 become DLL/DLM).
            self.wr_reg(Self::LCR, lcr_value | 1 << 7);

            // Set divisor least significant byte.
            self.wr_reg(Self::DLL, (divisor & 0xff) as u8);

            // Set divisor most significant byte.
            self.wr_reg(Self::DLM, (divisor >> 8) as u8);

            // Disable DLAB and set data word length to 8 bits.
            self.wr_reg(Self::LCR, lcr_value);
        }

        // Enable FIFO, clear TX/RX queues, and set interrupt watermark at 14 bytes.
        self.wr_reg(Self::FCR, 1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);

        // Mark data terminal ready, and signal request to send.
        self.wr_reg(Self::MCR, 1 << 1 | 1 << 0);

        // Enable receiver buffer interrupts (must be after DLAB is disabled,
        // since offset 1 is shared between IER and DLM).
        self.wr_reg(Self::IER, 1 << 0);
    }
}

impl Write for Uart16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        s.bytes().for_each(|b| {
            while !self.put(b) {
                core::hint::spin_loop();
            }
        });
        Ok(())
    }
}
