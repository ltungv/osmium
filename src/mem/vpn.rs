use core::{fmt, ops};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtPageNumber(usize);

impl VirtPageNumber {
    pub const BITS: usize = 27;

    pub const fn new_trunc(vpn: usize) -> Self {
        let mask = (1 << Self::BITS) - 1;
        Self(vpn & mask)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn indices(self) -> [usize; 3] {
        [self.0 & 0x1ff, self.0 >> 9 & 0x1ff, self.0 >> 18 & 0x1ff]
    }
}

impl ops::Add<usize> for VirtPageNumber {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::Sub<Self> for VirtPageNumber {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl fmt::Pointer for VirtPageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vpn@{:x}", self.0)
    }
}
