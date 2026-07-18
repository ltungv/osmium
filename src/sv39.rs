//! Functions and types for managing virtual pages.

use core::{fmt, ops::Add};

use crate::{
    align_value,
    frame::{self, FRAME_ORDER, FRAME_SIZE},
    print, println,
};

/// Errors occurs when working with the page table.
#[derive(Debug)]
pub enum TableError {
    /// There's no free memory page left.
    OutOfMemory,

    /// The page table is in an invalid state.
    InvalidState,
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "out of memory."),
            Self::InvalidState => write!(f, "invalid state"),
        }
    }
}

/// A 4096-byte struct containing entries that map virtual adresses to physical addresses.
#[derive(Debug)]
#[repr(C, align(4096))]
pub struct PageTable([TableEntry; 512]);

impl PageTable {
    /// Create a mapping between the given virtual address and physical address.
    pub fn map(
        &mut self,
        frame_allocator: &frame::Allocator,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        flags: EntryFlag,
        level: usize,
    ) -> Result<(), TableError> {
        // Make sure the read, write, and execute flags have been provided. Otherwise, we'll leak
        // memory and always create a page fault.
        assert!(flags.is_readable() || flags.is_writeable() || flags.is_executable());

        // Extract the virtual page numbers from the virtual address.
        let vpns = vaddr.vpns();

        // Assume the root page table is valid
        let mut entry = &mut self.0[vpns[2]];
        for vpn_next in vpns[level..2].iter().rev() {
            if !entry.get_flags().is_valid() {
                // Allocate a 4096-byte page to contain to page table and mark the page entry as
                // valid. Because every page is 4096-byte aligned, only the physical page number
                // needs to be stored instead of the entire address.
                let page = frame_allocator.zalloc(1).ok_or(TableError::OutOfMemory)?;
                *entry = entry
                    .set_address(page)
                    .set_flags(EntryFlag::default().set_valid(true));
            }

            // Go to the next entry.
            let table = entry.get_address() as *mut PageTable;
            entry = unsafe { &mut (*table).0[*vpn_next] };
        }

        *entry = entry.set_address(paddr).set_flags(flags.set_valid(true));
        Ok(())
    }

    /// Unmap the page table.
    pub fn unmap(&mut self, frame_allocator: &mut frame::Allocator) -> Result<(), TableError> {
        for entry_lvl2 in self.0.iter() {
            let entry_lvl2_flags = entry_lvl2.get_flags();
            if !entry_lvl2_flags.is_valid() || entry_lvl2_flags.is_leaf() {
                // Ignore invalid and leaf entry.
                continue;
            }
            // Get the page table.
            let table_lvl1_addr = entry_lvl2.get_address();
            let table_lvl1 = {
                let table = table_lvl1_addr as *mut PageTable;
                unsafe { table.as_mut().unwrap() }
            };
            // Since the number of levels is constant, we op for nesting loops instead of recursion
            // If we recursively call `unmap` again on inner tables, we would make extraneous
            // iterations when working on the level 0 table.
            for entry_lvl1 in table_lvl1.0.iter() {
                let entry_lvl1_flags = entry_lvl1.get_flags();
                if !entry_lvl1_flags.is_valid() || entry_lvl1_flags.is_leaf() {
                    // Ignore invalid and leaf entry.
                    continue;
                }
                let table_lvl0_addr = entry_lvl1.get_address();
                unsafe {
                    frame_allocator.dealloc(PhysAddr::from(table_lvl0_addr));
                }
            }
            unsafe {
                frame_allocator.dealloc(PhysAddr::from(table_lvl1_addr));
            }
        }
        Ok(())
    }

    /// Translate the given virtual address into its corresponding physical address.
    pub fn virt2phys(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        // Extract the virtual page numbers from the virtual address.
        let vpn_parts = vaddr.vpns();

        // Assume the root is valid
        let mut entry = &self.0[vpn_parts[2]];
        for i in (0..3).rev() {
            println!("probe {:b}", entry.0);
            let flags = entry.get_flags();
            if !flags.is_valid() {
                println!("invalid");
                break;
            }
            if flags.is_leaf() {
                // According to RISC-V, a leaf can be at any level.
                //
                // One thing to note is that only PPN[2:leaf-level] will be used to develop the
                // physical physical addres. For example, if level 2's (the top level) page table
                // entry is a leaf, then only PPN[2] contributes to the physical address. VPN[1]
                // is copied to PPN[1], VPN[0] is copied to PPN[0], and the page offset is copied,
                // as normal.
                //
                // The offset mask masks off the PPN. Each PPN is 9 bits and they start
                // at bit #12. So, our formula 12 + i * 9
                return Some(entry.translate(vaddr, i));
            }
            // Go to the next entry.
            let table = entry.get_address() as *mut PageTable;
            let vpn_next = vpn_parts[i - 1];
            entry = unsafe { &mut (*table).0[vpn_next] };
        }
        None
    }

    /// Performs identity map (vaddr == paddr) for addresses in the range [start, end].
    pub fn id_map_range(
        &mut self,
        frame_allocator: &frame::Allocator,
        start: usize,
        end: usize,
        flags: EntryFlag,
    ) -> Result<(), TableError> {
        let mut addr = start & !(FRAME_SIZE - 1);
        let num_kb_pages = (align_value(end, FRAME_ORDER) - addr) / FRAME_SIZE;
        println!("num_kb_pages={}", num_kb_pages);
        for _ in 0..num_kb_pages {
            self.map(frame_allocator, addr.into(), addr.into(), flags, 0)?;
            addr += FRAME_SIZE;
        }
        Ok(())
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self([TableEntry(0); 512])
    }
}

/// A page table entry as described in RISC-V Sv39's specifications.
#[derive(Debug, Default, Clone, Copy)]
pub struct EntryFlag(u8);

impl EntryFlag {
    const V_BIT: u8 = 1 << 0;
    const R_BIT: u8 = 1 << 1;
    const W_BIT: u8 = 1 << 2;
    const E_BIT: u8 = 1 << 3;
    const U_BIT: u8 = 1 << 4;
    const G_BIT: u8 = 1 << 5;
    const A_BIT: u8 = 1 << 6;
    const D_BIT: u8 = 1 << 7;

    fn is_valid(&self) -> bool {
        self.is_set(EntryFlag::V_BIT)
    }

    fn is_readable(&self) -> bool {
        self.is_set(EntryFlag::R_BIT)
    }

    fn is_writeable(&self) -> bool {
        self.is_set(EntryFlag::W_BIT)
    }

    fn is_executable(&self) -> bool {
        self.is_set(EntryFlag::E_BIT)
    }

    fn is_user_mode(&self) -> bool {
        self.is_set(EntryFlag::U_BIT)
    }

    fn is_global_mapping(&self) -> bool {
        self.is_set(EntryFlag::G_BIT)
    }

    fn is_accessed(&self) -> bool {
        self.is_set(EntryFlag::A_BIT)
    }

    fn is_dirty(&self) -> bool {
        self.is_set(EntryFlag::D_BIT)
    }

    fn is_leaf(&self) -> bool {
        self.is_readable() | self.is_writeable() | self.is_executable()
    }

    fn is_set(&self, bits: u8) -> bool {
        self.0 & bits != 0
    }

    fn set_valid(self, v: bool) -> Self {
        self.set(EntryFlag::V_BIT, v)
    }

    /// Set the R_BIT of the flag.
    pub fn set_readable(self, v: bool) -> Self {
        self.set(EntryFlag::R_BIT, v)
    }

    /// Set the W_BIT of the flag.
    pub fn set_writeable(self, v: bool) -> Self {
        self.set(EntryFlag::W_BIT, v)
    }

    /// Set the E_BIT of the flag.
    pub fn set_executable(self, v: bool) -> Self {
        self.set(EntryFlag::E_BIT, v)
    }

    fn set_user_mode(self, v: bool) -> Self {
        self.set(EntryFlag::U_BIT, v)
    }

    fn set_global_mapping(self, v: bool) -> Self {
        self.set(EntryFlag::G_BIT, v)
    }

    fn set_accessed(self, v: bool) -> Self {
        self.set(EntryFlag::A_BIT, v)
    }

    fn set_dirty(self, v: bool) -> Self {
        self.set(EntryFlag::D_BIT, v)
    }

    fn set(self, bits: u8, v: bool) -> Self {
        if v {
            Self(self.0 | bits)
        } else {
            Self(self.0 & !bits)
        }
    }
}

/// Representation of an entry in the allocation page table.
#[derive(Debug, Clone, Copy)]
pub struct TableEntry(usize);

impl TableEntry {
    fn translate(&self, vaddr: VirtAddr, lvl: usize) -> PhysAddr {
        let offset_mask = (1 << (12 + lvl * 9)) - 1;
        let offset = vaddr.0 & offset_mask;
        let ppns = self.get_address() & !offset_mask;
        PhysAddr(ppns | offset)
    }

    fn get_address(&self) -> usize {
        (self.0 & !0x3ff) << 2
    }

    fn set_address(self, addr: PhysAddr) -> Self {
        let ppns = addr.ppns();
        Self(self.0 | (ppns[2]) << 28 | (ppns[1]) << 19 | (ppns[0]) << 10)
    }

    fn get_flags(&self) -> EntryFlag {
        EntryFlag((self.0 & 0xff) as u8)
    }

    fn set_flags(self, flags: EntryFlag) -> Self {
        Self(self.0 | flags.0 as usize)
    }
}
/// A physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(usize);

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Phys({:x})", self.0)
    }
}

impl<T> From<*const T> for PhysAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for PhysAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl PhysAddr {
    /// The zero physical address.
    pub const ZERO: Self = Self(0);

    /// Decomposes the physical address into physical page numbers (PPNs).
    pub fn ppns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x3ff_ffff,
        ]
    }

    /// Offset in bytes between this physical address and another one.
    pub fn offset_from(self, other: Self) -> isize {
        self.0 as isize - other.0 as isize
    }

    /// Derives a raw pointer from this address.
    pub fn as_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

/// A virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(usize);

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Virt({:x})", self.0)
    }
}

impl<T> From<*const T> for VirtAddr {
    fn from(addr: *const T) -> Self {
        Self(addr.addr())
    }
}

impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl VirtAddr {
    /// The zero virtual address.
    pub const ZERO: Self = Self(0);

    /// Decompose the virtual address into virtual page numbers (VPNs).
    pub fn vpns(self) -> [usize; 3] {
        [
            self.0 >> 12 & 0x1ff,
            self.0 >> 21 & 0x1ff,
            self.0 >> 30 & 0x1ff,
        ]
    }
}
