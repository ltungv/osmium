//! Driver for uart devices.

use core::{
    error::Error,
    fmt::{self, Write},
    num::NonZeroU8,
    ptr::NonNull,
};

use crate::UART_ADDR;

/// Global uart device driver.
static UART_16550: spin::Once<spin::Mutex<Uart16550>> = spin::Once::new();

/// Print a formatted string using the global uart driver.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        $crate::uart::print(format_args!($($args)*));
    }};
}

/// Print a formatted string using the global uart driver, followed by a new line.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($($arg:tt)*) => ($crate::print!("{}\r\n", format_args!($($arg)*)));
}

/// Print using the global uart driver.
pub fn print(args: core::fmt::Arguments<'_>) {
    driver()
        .lock()
        .write_fmt(args)
        .expect("uart driver should print");
}

/// Get a reference to the global uart driver.
pub fn driver() -> &'static spin::Mutex<Uart16550> {
    UART_16550.call_once(|| {
        let mut uart = unsafe {
            let ptr = NonNull::new_unchecked(UART_ADDR as *mut u8);
            Uart16550::new(ptr, 1).expect("uart driver should be created")
        };
        uart.init();
        spin::Mutex::new(uart)
    })
}

/// A driver for 16550 UART devices backed by memory-mapped I/O addresses.
pub struct Uart16550 {
    ptr: NonNull<u8>,
    stride: NonZeroU8,
}

// SAFETY:
// * `Uart16550` is not `Sync`, so concurrent access from multiple thread is not possible without
//   additional synchronizations
// * the device address is ensured to points to a physical memory region large enough to accomodate
//   `num_registers` addresses, and access to the region is given exclusively to the driver instance
// * all operations take a `&mut self` which ensures the driver is only accessed by at most one
//   thread at a time
unsafe impl Send for Uart16550 {}

impl Write for Uart16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            while !self.try_put(b) {
                core::hint::spin_loop();
            }
        }
        Ok(())
    }
}

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

    unsafe fn new(ptr: NonNull<u8>, stride: u8) -> Result<Self, InvalidAddressError> {
        if !stride.is_power_of_two() {
            return Err(InvalidAddressError::InvalidStride(stride));
        }
        let Some(stride) = NonZeroU8::new(stride) else {
            return Err(InvalidAddressError::InvalidStride(stride));
        };
        if (ptr.as_ptr() as usize)
            .checked_add((Self::NUM_REGISTERS - 1) * stride.get() as usize)
            .is_none()
        {
            return Err(InvalidAddressError::InvalidAddress(ptr));
        }
        Ok(Self { ptr, stride })
    }

    /// Try to put a byte into the transmitter holding register, returning true if the byte has been
    /// successfully acknowledged.
    fn try_put(&mut self, byte: u8) -> bool {
        if self.read_from(Self::LSR) & (1 << 5) == 0 {
            return false;
        }
        self.write_to(Self::THR, byte);
        true
    }

    /// Try to get a byte from the receiver holding register, returning `None` if no byte is ready
    /// to be read.
    fn try_get(&mut self) -> Option<u8> {
        if self.read_from(Self::LSR) & (1 << 0) == 0 {
            return None;
        }
        Some(self.read_from(Self::RHR))
    }

    /// Read a byte from the register at the given offset.
    fn read_from(&mut self, offset: usize) -> u8 {
        unsafe {
            self.ptr
                .add(offset * self.stride.get() as usize)
                .read_volatile()
        }
    }

    /// Write a byte to the register at the given offset.
    fn write_to(&mut self, offset: usize, value: u8) {
        unsafe {
            self.ptr
                .add(offset * self.stride.get() as usize)
                .write_volatile(value);
        }
    }

    /// Initialize the UART device.
    fn init(&mut self) {
        // disable all interrupts during initialization
        self.write_to(Self::IER, 0);
        // data word length: 8 bits
        let lcr_value = 1 << 1 | 1 << 0;
        // set the divisor from a global clock rate of 22.729 mhz (22,729,000 cycles per second)
        // to a signaling rate of 2400 (baud).
        //
        // the formula given in the ns16500a specification for calculating the divisor is:
        // divisor = ceil((clock_hz) / (baud_sps x 16))
        // divisor = ceil(22_729_000 / (2400 x 16))
        // divisor = ceil(22_729_000 / 38_400)
        // divisor = ceil(591.901)
        // divisor = 592
        let divisor = 592u16;
        {
            // enable dlab to access the divisor latches (offsets 0 and 1 become dll/dlm)
            self.write_to(Self::LCR, lcr_value | 1 << 7);
            // set divisor least significant byte
            self.write_to(Self::DLL, (divisor & 0xff) as u8);
            // set divisor most significant byte
            self.write_to(Self::DLM, (divisor >> 8) as u8);
            // disable dlab and set data word length to 8 bits
            self.write_to(Self::LCR, lcr_value);
        }
        // enable fifo, clear tx/rx queues, and set interrupt watermark at 14 bytes
        self.write_to(Self::FCR, 1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);
        // mark data terminal ready, and signal request to send
        self.write_to(Self::MCR, 1 << 1 | 1 << 0);
        // enable receiver buffer interrupts (must be after dlab is disabled, since offset 1 is
        // shared between ier and dlm)
        self.write_to(Self::IER, 1 << 0);
    }
}

#[derive(Debug)]
enum InvalidAddressError {
    /// The given address is invalid.
    InvalidAddress(NonNull<u8>),

    /// The given stride is invalid.
    InvalidStride(u8),
}

impl Error for InvalidAddressError {}

impl fmt::Display for InvalidAddressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAddress(ptr) => {
                write!(f, "{ptr:p} is not a valid UART device address")
            }
            Self::InvalidStride(stride) => write!(
                f,
                "stride must be non-zero and a power of two; got {stride}"
            ),
        }
    }
}
