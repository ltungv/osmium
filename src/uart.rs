//! Driver for uart devices.

use core::{
    fmt::{self, Write},
    ptr::NonNull,
};

use crate::spinlock::Spinlock;

/// Address of the UART device on the `virt` machine in `QEMU`
pub const UART_BASE: usize = 0x1000_0000;

/// Print a formatted string using the global uart console.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        $crate::uart::print(format_args!($($args)*));
    }};
}

/// Print a formatted string using the global uart console, followed by a new line.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($($arg:tt)*) => ($crate::print!("{}\r\n", format_args!($($arg)*)));
}

static CONSOLE: Spinlock<Console> = Spinlock::new(
    "console",
    Console(Uart16550::new(Config {
        interrupts: IER::RHR_READY,
        frequency: 1_843_200,
        prescaler_division_factor: None,
        fifo_trigger_level: Some(FifoTriggerLevel::Lvl14),
        baud_rate: 9600,
        data_bits: WordLength::Bits8,
        extra_stop_bits: false,
        parity: Parity::Disabled,
    })),
);

/// Initializes the global UART console.
pub fn init() {
    let ptr = unsafe { NonNull::new_unchecked(UART_BASE as *mut u8) };
    unsafe {
        CONSOLE
            .lock()
            .0
            .init(ptr)
            .expect("driver should initialize")
    }
}

/// Print using the global uart console.
pub fn print(args: core::fmt::Arguments<'_>) {
    let mut console = CONSOLE.lock();
    console.write_fmt(args).expect("uart console should print");
}

struct Console(Uart16550);

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut i = 0;
        while i < s.len() {
            i += self.0.send_bytes(&s.as_bytes()[i..])
        }
        Ok(())
    }
}

#[derive(Debug)]
enum Error {
    BadAddress(NonNull<u8>),
    BadBaudRate,
    NoDevice,
}

impl core::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadAddress(ptr) => {
                write!(f, "bad address {ptr:p}")
            }
            Error::NoDevice => write!(f, "no device"),
            Error::BadBaudRate => write!(f, "bad baud rate"),
        }
    }
}

struct Config {
    interrupts: IER,
    frequency: u32,
    prescaler_division_factor: Option<u32>,
    fifo_trigger_level: Option<FifoTriggerLevel>,
    baud_rate: u32,
    data_bits: WordLength,
    extra_stop_bits: bool,
    parity: Parity,
}

impl Config {
    fn divisor_latch(&self) -> Option<(u8, u8)> {
        let divisor = calc_divisor(
            self.frequency,
            self.baud_rate,
            self.prescaler_division_factor,
        )?;
        let dll = divisor & 0xff;
        let dlm = divisor >> 8;
        Some((dll as u8, dlm as u8))
    }
}

struct Uart16550 {
    cfg: Config,
    ptr: NonNull<u8>,
}

unsafe impl Send for Uart16550 {}

impl Uart16550 {
    const fn new(cfg: Config) -> Self {
        Self {
            cfg,
            ptr: NonNull::dangling(),
        }
    }
}

impl Uart16550 {
    unsafe fn init(&mut self, ptr: NonNull<u8>) -> Result<(), Error> {
        self.ptr = ptr;
        if (self.ptr.as_ptr() as usize).checked_add(7).is_none() {
            return Err(Error::BadAddress(ptr));
        }
        {
            let mut check_fn = |write| {
                // SAFETY: We operate on valid register addresses.
                let read = unsafe {
                    self.write(Register::Spr, write);
                    self.read(Register::Spr)
                };
                if read != write {
                    return Err(Error::NoDevice);
                }
                Ok(())
            };
            check_fn(0x42)?;
            check_fn(0x73)?;
        }
        unsafe {
            self.write(Register::Ier, 0);
        }
        unsafe {
            let (dll, dlm) = self.cfg.divisor_latch().ok_or(Error::BadBaudRate)?;
            self.write(Register::Lcr, LCR::DLAB.bits());
            self.write(Register::Dll, dll);
            self.write(Register::Dlm, dlm);
            self.write(Register::Lcr, 0);
        }
        unsafe {
            let mut lcr = LCR::empty();
            lcr = lcr.with_parity(self.cfg.parity);
            lcr = lcr.with_word_length(self.cfg.data_bits);
            if self.cfg.extra_stop_bits {
                lcr |= LCR::MORE_STOP_BITS;
            }
            self.write(Register::Lcr, lcr.bits());
        }
        unsafe {
            let mut fcr = FCR::empty();
            fcr |= FCR::RX_FIFO_RESET;
            fcr |= FCR::TX_FIFO_RESET;
            if let Some(lvl) = self.cfg.fifo_trigger_level {
                fcr |= FCR::FIFO_ENABLE;
                fcr = fcr.with_fifo_trigger_level(lvl);
            }
            self.write(Register::Fcr, fcr.bits());
        }
        unsafe {
            let mut mcr = MCR::empty();
            mcr |= MCR::DTR;
            mcr |= MCR::RTS;
            mcr |= MCR::OUT2_INTR_ENABLE;
            self.write(Register::Mcr, mcr.bits());
        }
        unsafe {
            self.write(Register::Ier, self.cfg.interrupts.bits());
        }
        loop {
            let lsr = LSR::from_bits_retain(unsafe { self.read(Register::Lsr) });
            if lsr.contains(LSR::TRANSMITTER_EMPTY) {
                break;
            } else {
                core::hint::spin_loop()
            }
        }
        Ok(())
    }

    unsafe fn read(&mut self, register: Register) -> u8 {
        let ptr = unsafe { self.ptr.add(register.offset()) };
        unsafe { ptr.read_volatile() }
    }

    unsafe fn write(&mut self, register: Register, bits: u8) {
        let ptr = unsafe { self.ptr.add(register.offset()) };
        unsafe { ptr.write_volatile(bits) }
    }

    fn ready_to_recv(&mut self) -> bool {
        let lsr = LSR::from_bits_retain(unsafe { self.read(Register::Lsr) });
        lsr.contains(LSR::DATA_READY)
    }

    fn ready_to_send(&mut self) -> bool {
        let lsr = LSR::from_bits_retain(unsafe { self.read(Register::Lsr) });
        let mcr = MCR::from_bits_retain(unsafe { self.read(Register::Mcr) });
        let msr = MSR::from_bits_retain(unsafe { self.read(Register::Msr) });
        lsr.contains(LSR::THR_EMPTY) && (mcr.contains(MCR::LOOP_BACK) || msr.contains(MSR::CTS))
    }

    fn recv_bytes(&mut self, buffer: &mut [u8]) -> usize {
        let mut bytes_received = 0;
        for b in buffer {
            if !self.ready_to_recv() {
                break;
            }
            *b = unsafe { self.read(Register::Rhr) };
            bytes_received += 1;
        }
        bytes_received
    }

    fn send_bytes(&mut self, buffer: &[u8]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        if !self.ready_to_send() {
            return 0;
        }
        let fifo_enabled = self.cfg.fifo_trigger_level.is_some();
        let bytes = if fifo_enabled {
            let max_index = buffer.len().min(16);
            &buffer[..max_index]
        } else {
            &buffer[..1]
        };
        for &byte in bytes {
            unsafe {
                self.write(Register::Thr, byte);
            }
        }
        bytes.len()
    }
}

fn calc_divisor(freq: u32, baud: u32, prescaler_division_factor: Option<u32>) -> Option<u16> {
    let psd = prescaler_division_factor.unwrap_or_default();
    let x = 16 * (psd + 1) * baud;
    if !freq.is_multiple_of(x) {
        return None;
    }
    Some(u16::try_from(freq / x).expect("divisor value should fits in 16 bits"))
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum FifoTriggerLevel {
    Lvl1 = 0b00,
    Lvl4 = 0b01,
    Lvl8 = 0b10,
    Lvl14 = 0b11,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum WordLength {
    Bits5 = 0b00,
    Bits6 = 0b01,
    Bits7 = 0b10,
    Bits8 = 0b11,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Parity {
    Disabled = 0b000,
    Odd = 0b001,
    Even = 0b011,
    Forced1 = 0b101,
    Forced0 = 0b111,
}

#[derive(Debug, Clone, Copy)]
enum Register {
    Rhr,
    Thr,
    Ier,
    Isr,
    Fcr,
    Lcr,
    Mcr,
    Lsr,
    Msr,
    Spr,
    Dll,
    Dlm,
    Psd,
}

impl Register {
    const fn offset(self) -> usize {
        match self {
            Self::Rhr => 0b000,
            Self::Thr => 0b000,
            Self::Ier => 0b001,
            Self::Isr => 0b010,
            Self::Fcr => 0b010,
            Self::Lcr => 0b011,
            Self::Mcr => 0b100,
            Self::Lsr => 0b101,
            Self::Msr => 0b110,
            Self::Spr => 0b111,
            Self::Dll => 0b000,
            Self::Dlm => 0b001,
            Self::Psd => 0b101,
        }
    }
}

bitflags::bitflags! {
    /// Interrupt Enable Register.
    struct IER: u8 {
        const RHR_READY = 1 << 0;
        const THR_EMPTY = 1 << 1;
        const RECEIVER_LINE_STATUS = 1 << 2;
        const MODEM_STATUS = 1 << 3;
        const DMA_RX_END = 1 << 6;
        const DMA_TX_END = 1 << 7;
    }
}

bitflags::bitflags! {
    /// Interrupt Status Register.
    struct ISR: u8 {
        const INTR_STATUS = 1 << 0;
        const INTR_ID0 = 1 << 1;
        const INTR_ID1 = 1 << 2;
        const INTR_ID2 = 1 << 3;
        const DMA_RX_END = 1 << 4;
        const DMA_TX_END = 1 << 5;
        const FIFOS_ENABLED0 = 1 << 6;
        const FIFOS_ENABLED1 = 1 << 7;
    }
}

bitflags::bitflags! {
    /// FIFO Control Register.
    struct FCR: u8 {
        const FIFO_ENABLE = 1 << 0;
        const RX_FIFO_RESET = 1 << 1;
        const TX_FIFO_RESET = 1 << 2;
        const DMA_MODE = 1 << 3;
        const ENABLE_DMA_END = 1 << 4;
        const RECEIVER_FIFO_TRIGGER_LVL0 = 1 << 6;
        const RECEIVER_FIFO_TRIGGER_LVL1 = 1 << 7;
    }
}

impl FCR {
    fn with_fifo_trigger_level(mut self, lvl: FifoTriggerLevel) -> Self {
        let mask = Self::RECEIVER_FIFO_TRIGGER_LVL0 | Self::RECEIVER_FIFO_TRIGGER_LVL1;
        let bits = (lvl as u8) << 6;
        self &= !mask;
        self |= Self::from_bits_retain(bits);
        self
    }
}

bitflags::bitflags! {
    /// Line Control Register.
    struct LCR: u8 {
        const WORD_LEN0 = 1 << 0;
        const WORD_LEN1 = 1 << 1;
        const MORE_STOP_BITS = 1 << 2;
        const PARITY_ENABLE = 1 << 3;
        const EVEN_PARITY = 1 << 4;
        const FORCE_PARITY = 1 << 5;
        const SET_BREAK = 1 << 6;
        const DLAB = 1 << 7;
    }
}

impl LCR {
    fn with_parity(mut self, parity: Parity) -> Self {
        let mask = Self::PARITY_ENABLE | Self::EVEN_PARITY | Self::FORCE_PARITY;
        let bits = (parity as u8) << 3;
        self &= !mask;
        self |= Self::from_bits_retain(bits);
        self
    }

    fn with_word_length(mut self, wordlen: WordLength) -> Self {
        let mask = Self::WORD_LEN0 | Self::WORD_LEN1;
        let bits = wordlen as u8;
        self &= !mask;
        self |= Self::from_bits_retain(bits);
        self
    }
}

bitflags::bitflags! {
    struct MCR: u8 {
        const DTR = 1 << 0;
        const RTS = 1 << 1;
        const OUT1 = 1 << 2;
        const OUT2_INTR_ENABLE = 1 << 3;
        const LOOP_BACK = 1 << 4;
    }
}

bitflags::bitflags! {
    struct LSR: u8 {
       const DATA_READY = 1 << 0;
       const OVERRUN_ERROR = 1 << 1;
       const PARITY_ERROR = 1 << 2;
       const FRAMING_ERROR = 1 << 3;
       const BREAK_INTR = 1 << 4;
       const THR_EMPTY = 1 << 5;
       const TRANSMITTER_EMPTY = 1 << 6;
       const FIFO_DATA_ERROR = 1 << 7;
   }
}

bitflags::bitflags! {
    struct MSR: u8 {
        const DELTA_CTS = 1 << 0;
        const DELTA_DSR = 1 << 1;
        const TRAILING_EDGE_RI = 1 << 2;
        const DELTA_CD = 1 << 3;
        const CTS = 1 << 4;
        const DSR = 1 << 5;
        const RI = 1 << 6;
        const CD = 1 << 7;
    }
}

bitflags::bitflags! {
    struct PSD: u8 {
        const FACTOR0 = 1 << 0;
        const FACTOR1 = 1 << 1;
        const FACTOR2 = 1 << 2;
        const FACTOR3 = 1 << 3;
    }
}
