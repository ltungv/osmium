//! Page-based virtual memory system.

pub mod page_table;
pub mod sv39;

use core::{arch::asm, fmt};

use crate::{
    Error,
    kalloc::Kmem,
    mem::{
        BSS_ADDR, DATA_ADDR, HEAP_ADDR, MEM_ADDR, MEM_SIZE, PAGE_SIZE, RODATA_ADDR, STACK_ADDR,
        TRAMP_ADDR, paddr::PhysAddr, vaddr::VirtAddr,
    },
    paging::{page_table::PteFlags, sv39::Sv39},
    uart::UART_BASE,
};

/// Initializes the kernel's page table for this current hardware thread.
///
/// Upon the first call to this function, a fresh kernel's page table is created containing the
/// direct mapping of all useable physical memory. Multiple concurrent calls to this function are
/// safe, such that at most one kernel's page table will ever be created.
///
/// Upon concurrent calls from multiple hardware threads, only a single hardware thread can be in
/// charge of setting up the kernel's page table. All other hardware threads wait for the setup to
/// finish before updating their local `satp` register to the address of the newly created kernel's
/// page table.
pub fn kvminit() {
    static KVM: spin::Once<MappedPageTable> = spin::Once::new();
    let kvm = KVM.call_once(|| {
        let kmem = Kmem::get();
        let mut table = Sv39::new(kmem).expect("physical memory should be available");
        let mut mapdirect = |addr: usize, size: usize, flags: PteFlags| -> Result<(), Error> {
            table.map(VirtAddr::new(addr), PhysAddr::new(addr), size, flags, kmem)
        };

        mapdirect(UART_BASE, PAGE_SIZE, PteFlags::R | PteFlags::W)
            .expect("uart registers should be mapped");

        unsafe {
            mapdirect(MEM_ADDR, TRAMP_ADDR - MEM_ADDR, PteFlags::R | PteFlags::X)
                .expect(".text section should be mapped");

            mapdirect(
                TRAMP_ADDR,
                RODATA_ADDR - TRAMP_ADDR,
                PteFlags::R | PteFlags::X,
            )
            .expect(".tramp section should be mapped");

            mapdirect(
                RODATA_ADDR,
                DATA_ADDR - RODATA_ADDR,
                PteFlags::R | PteFlags::X,
            )
            .expect(".rodata section should be mapped");

            mapdirect(DATA_ADDR, BSS_ADDR - DATA_ADDR, PteFlags::R | PteFlags::W)
                .expect(".data section should be mapped");

            mapdirect(BSS_ADDR, STACK_ADDR - BSS_ADDR, PteFlags::R | PteFlags::W)
                .expect(".bss section should be mapped");

            mapdirect(
                STACK_ADDR,
                HEAP_ADDR - STACK_ADDR,
                PteFlags::R | PteFlags::W,
            )
            .expect("kernel's stack section should be mapped");

            mapdirect(
                HEAP_ADDR,
                (MEM_ADDR + MEM_SIZE) - HEAP_ADDR,
                PteFlags::R | PteFlags::W,
            )
            .expect("kernel's heap section should be mapped");
        }
        MappedPageTable::new(table)
    });
    unsafe {
        // wait for any previous writes to the page table memory to finish
        asm!("sfence.vma");
        // write to the satp register
        asm!("csrw satp, {}", in(reg) kvm.satp());
        // flush stale entries from the translation lookaside buffer
        asm!("sfence.vma");
    }
}

/// A high-level abstraction over a RISC-V page table.
///
/// It encapsulates the root physical page number and provides safe methods
/// to map and unmap virtual addresses to physical addresses using a 3-level
/// radix tree (Sv39).
pub struct MappedPageTable<'t>(spin::Mutex<Sv39<'t>>);

impl<'t> MappedPageTable<'t> {
    fn new(page_table: Sv39<'t>) -> Self {
        Self(spin::Mutex::new(page_table))
    }

    fn satp(&self) -> usize {
        let vm = self.0.lock();
        vm.satp()
    }

    /// Translate the given virtual address into the physical address that was mapped to it.
    pub fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        let vm = self.0.lock();
        vm.translate(vaddr)
    }

    /// Map the given virtual address to the given physical address.
    pub fn map(
        &self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        flags: PteFlags,
        kmem: &Kmem,
    ) -> Result<(), Error> {
        let mut vm = self.0.lock();
        vm.map(vaddr, paddr, size, flags, kmem)
    }

    /// Unmap all previously mapped virtual addresses.
    pub fn unmap(&mut self, kmem: &Kmem) {
        let mut vm = self.0.lock();
        vm.unmap(kmem);
    }
}

/// Error from mapping virtual addresses to physical addresses.
#[derive(Debug)]
pub enum MappingError {
    /// The given virtual address is invalid.
    BadAddress(VirtAddr),

    /// The given size is invalid.
    BadSize(usize),

    /// The given virutal address has been mapped.
    Remap(VirtAddr),
}

impl core::error::Error for MappingError {}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadAddress(addr) => {
                write!(f, "{addr:p} is not a valid virtual address")
            }
            Self::BadSize(size) => write!(
                f,
                "size must be non-zero and aligned to {PAGE_SIZE}; got {size}"
            ),
            Self::Remap(addr) => {
                write!(f, "{addr:p} is remapped to a different physical address")
            }
        }
    }
}
