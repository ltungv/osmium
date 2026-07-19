//! Functions and types for managing virtual pages.

use core::{fmt, ops::Add};

use bitflags::bitflags;

use crate::{
    align_value,
    frame::{FRAME_ORDER, FRAME_SIZE, FrameAllocator, FrameId},
};

/// Errors occurs when working with the page table.
#[derive(Debug)]
pub enum Error {
    /// There's no free memory page left.
    OutOfMemory,

    /// The page table is in an invalid state.
    InvalidState,
}

impl fmt::Display for Error {
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
        frame_allocator: &FrameAllocator,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        flags: EntryFlags,
        level: usize,
    ) -> Result<(), Error> {
        // Make sure the read, write, and execute flags have been provided. Otherwise, we'll leak
        // memory and always create a page fault.
        assert!(flags.is_leaf());

        // Extract the virtual page numbers from the virtual address.
        let vpns = vaddr.vpns();

        // Assume the root page table is valid
        let mut entry = &mut self.0[vpns[2]];
        for vpn_next in vpns[level..2].iter().rev() {
            if !entry.flags().contains(EntryFlags::VALID) {
                // Allocate a 4096-byte page to contain to page table and mark the page entry as
                // valid. Because every page is 4096-byte aligned, only the physical page number
                // needs to be stored instead of the entire address.
                let page = frame_allocator.zalloc(1).ok_or(Error::OutOfMemory)?;
                *entry = TableEntry::new(page.addr().into(), EntryFlags::VALID);
            }

            // Go to the next entry.
            let table = entry.addr() as *mut PageTable;
            entry = unsafe { &mut (*table).0[*vpn_next] };
        }

        *entry = TableEntry::new(paddr, flags | EntryFlags::VALID);
        Ok(())
    }

    /// Unmap the page table.
    pub fn unmap(&mut self, frame_allocator: &FrameAllocator) -> Result<(), Error> {
        for entry_lvl2 in self.0.iter() {
            let entry_lvl2_flags = entry_lvl2.flags();
            if !entry_lvl2_flags.contains(EntryFlags::VALID) || entry_lvl2_flags.is_leaf() {
                // Ignore invalid and leaf entry.
                continue;
            }
            // Get the page table.
            let table_lvl1_addr = entry_lvl2.addr();
            let table_lvl1 = {
                let table = table_lvl1_addr as *mut PageTable;
                unsafe { table.as_mut().unwrap() }
            };
            // Since the number of levels is constant, we op for nesting loops instead of recursion
            // If we recursively call `unmap` again on inner tables, we would make extraneous
            // iterations when working on the level 0 table.
            for entry_lvl1 in table_lvl1.0.iter() {
                let entry_lvl1_flags = entry_lvl1.flags();
                if !entry_lvl1_flags.contains(EntryFlags::VALID) || entry_lvl1_flags.is_leaf() {
                    // Ignore invalid and leaf entry.
                    continue;
                }
                let table_lvl0_addr = entry_lvl1.addr();
                unsafe {
                    frame_allocator
                        .dealloc(FrameId::try_from(table_lvl0_addr).expect("valid frame address"));
                }
            }
            unsafe {
                frame_allocator
                    .dealloc(FrameId::try_from(table_lvl1_addr).expect("valid frame address"));
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
            let flags = entry.flags();
            if !flags.contains(EntryFlags::VALID) {
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
            let table = entry.addr() as *mut PageTable;
            let vpn_next = vpn_parts[i - 1];
            entry = unsafe { &mut (*table).0[vpn_next] };
        }
        None
    }

    /// Performs identity map (vaddr == paddr) for addresses in the range [start, end].
    pub fn id_map_range(
        &mut self,
        frame_allocator: &FrameAllocator,
        start: usize,
        end: usize,
        flags: EntryFlags,
    ) -> Result<(), Error> {
        let mut addr = start & !(FRAME_SIZE - 1);
        let num_kb_pages = (align_value(end, FRAME_ORDER) - addr) / FRAME_SIZE;
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

bitflags! {
    /// Flags for each page table entry.
    #[derive(Clone, Copy)]
    pub struct EntryFlags: u8 {
        /// Entry V_BIT flags.
        const VALID = 1 << 0;

        /// Entry R_BIT flags.
        const READ = 1 << 1;

        /// Entry W_BIT flags.
        const WRITE = 1 << 2;

        /// Entry X_BIT flags.
        const EXECUTE = 1 << 3;

        /// Entry U_BIT flags.
        const USER = 1 << 4;

        /// Entry G_BIT flags.
        const GLOBAL = 1 << 5;

        /// Entry A_BIT flags.
        const ACCESS = 1 << 6;

        /// Entry D_BIT flags.
        const DIRTY = 1 << 7;
    }
}

impl EntryFlags {
    /// Returns true if any one of the READ, WRITE, or EXECUTE flags is enable.
    pub fn is_leaf(&self) -> bool {
        self.intersects(Self::READ | Self::WRITE | Self::EXECUTE)
    }
}

/// Representation of an entry in the allocation page table.
#[derive(Debug, Clone, Copy)]
pub struct TableEntry(usize);

impl TableEntry {
    fn new(addr: PhysAddr, flags: EntryFlags) -> Self {
        let ppns = addr.ppns();
        Self(ppns[2] << 28 | ppns[1] << 19 | ppns[0] << 10 | flags.bits() as usize)
    }

    fn translate(&self, vaddr: VirtAddr, lvl: usize) -> PhysAddr {
        let offset_mask = (1 << (12 + lvl * 9)) - 1;
        let offset = vaddr.0 & offset_mask;
        let ppns = self.addr() & !offset_mask;
        PhysAddr(ppns | offset)
    }

    fn addr(&self) -> usize {
        (self.0 & !0x3ff) << 2
    }

    fn flags(&self) -> EntryFlags {
        let bits = self.0 & 0xff;
        EntryFlags::from_bits_retain(bits as u8)
    }
}
/// A physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(usize);

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
}

/// A virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(usize);

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
