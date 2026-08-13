//! A console for sending and receiving bytes to and from 16550 UART devices.

use core::{
    error::Error,
    fmt::{self, Write},
    num::NonZeroU8,
    ptr::NonNull,
};

use crate::addr::{PhysAddr, phys_to_virt};

/// Address of the UART device on the `virt` machine in `QEMU`.
pub const QEMU_ADDR: PhysAddr = PhysAddr::new_trunc(0x1000_0000);

static UART_16550: spin::Once<spin::Mutex<Uart16550>> = spin::Once::new();

/// Print a formatted string to the global console.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        $crate::uart::print(format_args!($($args)*));
    }};
}

/// Print a formatted string to the global console, with a new line.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($($arg:tt)*) => ($crate::print!("{}\r\n", format_args!($($arg)*)));
}

/// Print to the global console.
pub fn print(args: core::fmt::Arguments<'_>) {
    driver()
        .lock()
        .write_fmt(args)
        .expect("16550 UART driver should print");
}

/// Get a reference to the global console connected to the default address of the UART device on
/// the `virt` machine in `QEMU`.
pub fn driver() -> &'static spin::Mutex<Uart16550> {
    UART_16550.call_once(|| {
        let mut uart = unsafe {
            let addr = NonNull::new_unchecked(phys_to_virt(QEMU_ADDR).as_ptr_mut());
            Uart16550::new(addr, 1).expect("16550 UART driver should be created")
        };
        uart.init();
        spin::Mutex::new(uart)
    })
}

/// A driver for 16550 UART devices backed by memory-mapped I/O addresses.
pub struct Uart16550 {
    addr: NonNull<u8>,
    stride: NonZeroU8,
}

// SAFETY: `Uart16550` is not `Sync`, so concurrent access from multiple thread is not possible
// without additional synchronizations. The device address is ensured to points to a physical memory
// region large enough to accomodate `NUM_REGISTERS` addresses, and access to the region is given
// exclusively to the driver instance. All operations take a `&mut self` which ensures the driver is
// only accessed by at most one thread at a time.
unsafe impl Send for Uart16550 {}

impl Write for Uart16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        s.bytes().for_each(|b| {
            while !self.try_put(b) {
                core::hint::spin_loop();
            }
        });
        Ok(())
    }
}

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

    unsafe fn new(addr: NonNull<u8>, stride: u8) -> Result<Self, InvalidAddressError> {
        if !stride.is_power_of_two() {
            return Err(InvalidAddressError::InvalidStride(stride));
        }
        let Some(stride) = NonZeroU8::new(stride) else {
            return Err(InvalidAddressError::InvalidStride(stride));
        };
        if (addr.as_ptr() as usize)
            .checked_add((Self::NUM_REGISTERS - 1) * stride.get() as usize)
            .is_none()
        {
            return Err(InvalidAddressError::InvalidAddr(addr));
        }
        Ok(Self { addr, stride })
    }

    /// Try to put a byte into the transmitter holding register, returning true if the byte has been
    /// successfully acknowledged.
    fn try_put(&mut self, byte: u8) -> bool {
        if self.read_from(Self::LSR) & (1 << 5) == 0 {
            false
        } else {
            self.write_to(Self::THR, byte);
            true
        }
    }

    /// Try to get a byte from the receiver holding register, returning `None` if no byte is ready
    /// to be read.
    fn try_get(&mut self) -> Option<u8> {
        if self.read_from(Self::LSR) & (1 << 0) == 0 {
            None
        } else {
            Some(self.read_from(Self::RHR))
        }
    }

    /// Read a byte from a register offset
    fn read_from(&mut self, offset: usize) -> u8 {
        unsafe {
            self.addr
                .add(offset * self.stride.get() as usize)
                .read_volatile()
        }
    }

    /// Write a byte to a register offset
    fn write_to(&mut self, offset: usize, value: u8) {
        unsafe {
            self.addr
                .add(offset * self.stride.get() as usize)
                .write_volatile(value);
        }
    }

    /// Initialize the UART device.
    fn init(&mut self) {
        // Disable all interrupts during initialization.
        self.write_to(Self::IER, 0);

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
            self.write_to(Self::LCR, lcr_value | 1 << 7);

            // Set divisor least significant byte.
            self.write_to(Self::DLL, (divisor & 0xff) as u8);

            // Set divisor most significant byte.
            self.write_to(Self::DLM, (divisor >> 8) as u8);

            // Disable DLAB and set data word length to 8 bits.
            self.write_to(Self::LCR, lcr_value);
        }

        // Enable FIFO, clear TX/RX queues, and set interrupt watermark at 14 bytes.
        self.write_to(Self::FCR, 1 << 7 | 1 << 6 | 1 << 2 | 1 << 1 | 1 << 0);

        // Mark data terminal ready, and signal request to send.
        self.write_to(Self::MCR, 1 << 1 | 1 << 0);

        // Enable receiver buffer interrupts (must be after DLAB is disabled,
        // since offset 1 is shared between IER and DLM).
        self.write_to(Self::IER, 1 << 0);
    }
}

#[derive(Debug)]
enum InvalidAddressError {
    /// The given address is invalid, e.g., it can't accomodate [`NUM_REGISTERS`]
    /// consecutive addresses.
    InvalidAddr(NonNull<u8>),

    /// The given stride is invalid. A stride must be non-zero and a power of two.
    InvalidStride(u8),
}

impl Error for InvalidAddressError {}

impl fmt::Display for InvalidAddressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAddr(addr) => {
                write!(f, "{addr:p} is not a valid 16550 UART device address")
            }
            Self::InvalidStride(stride) => write!(
                f,
                "stride must be non-zero and a power of two; got {stride}"
            ),
        }
    }
}
