//! Page-based virtual memory system.

pub mod page_table;
pub mod sv39;

use core::fmt;

use crate::{
    Error,
    kalloc::{self, Kmem},
    mem::{
        BSS_ADDR, DATA_ADDR, HEAP_ADDR, MEM_ADDR, MEM_SIZE, PAGE_SIZE, RODATA_ADDR, STACK_ADDR,
        TRAMP_ADDR, TRAMPOLINE, paddr::PhysAddr, vaddr::VirtAddr,
    },
    paging::{page_table::PteFlags, sv39::Sv39},
    riscv::{
        asm::sfence_vma_all,
        registers::satp::{self, Satp},
    },
    spinlock::Spinlock,
    uart::UART_BASE,
};

static KVM: Spinlock<PageTableMap> = Spinlock::new(PageTableMap(None));

/// Gets a reference to the kernel's page table.
pub fn kvm() -> &'static Spinlock<PageTableMap<'static>> {
    &KVM
}

/// Initializes the kernel's page table for this current hardware thread.
pub fn kvminit() {
    let mut kmem = kalloc::kmem().lock();
    let mut kvm = KVM.lock();

    #[cfg(test)]
    kvm.map(
        VirtAddr::new(crate::test::SIFIVE_BASE),
        PhysAddr::new(crate::test::SIFIVE_BASE),
        PAGE_SIZE,
        PteFlags::R | PteFlags::W,
        &mut kmem,
    )
    .expect("sifive test registers should be mapped");

    kvm.map(
        VirtAddr::new(UART_BASE),
        PhysAddr::new(UART_BASE),
        PAGE_SIZE,
        PteFlags::R | PteFlags::W,
        &mut kmem,
    )
    .expect("uart registers should be mapped");

    unsafe {
        kvm.map(
            VirtAddr::new(MEM_ADDR),
            PhysAddr::new(MEM_ADDR),
            TRAMP_ADDR - MEM_ADDR,
            PteFlags::R | PteFlags::X,
            &mut kmem,
        )
        .expect(".text section should be mapped");

        kvm.map(
            VirtAddr::new(TRAMP_ADDR),
            PhysAddr::new(TRAMP_ADDR),
            RODATA_ADDR - TRAMP_ADDR,
            PteFlags::R | PteFlags::X,
            &mut kmem,
        )
        .expect(".tramp section should be mapped");

        kvm.map(
            VirtAddr::new(RODATA_ADDR),
            PhysAddr::new(RODATA_ADDR),
            DATA_ADDR - RODATA_ADDR,
            PteFlags::R | PteFlags::X,
            &mut kmem,
        )
        .expect(".rodata section should be mapped");

        kvm.map(
            VirtAddr::new(DATA_ADDR),
            PhysAddr::new(DATA_ADDR),
            BSS_ADDR - DATA_ADDR,
            PteFlags::R | PteFlags::W,
            &mut kmem,
        )
        .expect(".data section should be mapped");

        kvm.map(
            VirtAddr::new(BSS_ADDR),
            PhysAddr::new(BSS_ADDR),
            STACK_ADDR - BSS_ADDR,
            PteFlags::R | PteFlags::W,
            &mut kmem,
        )
        .expect(".bss section should be mapped");

        kvm.map(
            VirtAddr::new(STACK_ADDR),
            PhysAddr::new(STACK_ADDR),
            HEAP_ADDR - STACK_ADDR,
            PteFlags::R | PteFlags::W,
            &mut kmem,
        )
        .expect("kernel's stack section should be mapped");

        kvm.map(
            VirtAddr::new(HEAP_ADDR),
            PhysAddr::new(HEAP_ADDR),
            (MEM_ADDR + MEM_SIZE) - HEAP_ADDR,
            PteFlags::R | PteFlags::W,
            &mut kmem,
        )
        .expect("kernel's heap section should be mapped");

        kvm.map(
            VirtAddr::new(TRAMPOLINE),
            PhysAddr::new(TRAMP_ADDR),
            PAGE_SIZE,
            PteFlags::R | PteFlags::X,
            &mut kmem,
        )
        .expect("trampoline should be mapped");
    }
}

/// Enable paging using the kernel's page table.
pub fn kvminithart() {
    unsafe {
        // wait for any previous writes to the page table memory to finish
        sfence_vma_all();
        // write to the satp register
        satp::write(KVM.lock().satp());
        // flush stale entries from the translation lookaside buffer
        sfence_vma_all();
    }
}

/// A high-level abstraction over a RISC-V page table.
///
/// It encapsulates the root physical page number and provides safe methods
/// to map and unmap virtual addresses to physical addresses using a 3-level
/// radix tree (Sv39).
pub struct PageTableMap<'t>(Option<Sv39<'t>>);

impl<'t> PageTableMap<'t> {
    fn satp(&self) -> Satp {
        self.0.as_ref().map_or_else(Satp::bare, Sv39::satp)
    }

    fn root(&mut self, kmem: &mut Kmem) -> Result<&mut Sv39<'t>, Error> {
        let vm = if let Some(vm) = self.0.as_mut() {
            vm
        } else {
            let sv39 = Sv39::new(kmem)?;
            self.0.insert(sv39)
        };
        Ok(vm)
    }

    /// Translate the given virtual address into the physical address that was mapped to it.
    pub fn translate(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        self.0.as_ref().and_then(|vm| vm.translate(vaddr))
    }

    /// Map the given virtual address to the given physical address.
    pub fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        flags: PteFlags,
        kmem: &mut Kmem,
    ) -> Result<(), Error> {
        let root = self.root(kmem)?;
        root.map(vaddr, paddr, size, flags, kmem)
    }

    /// Unmap all previously mapped virtual addresses.
    pub fn unmap(&mut self, kmem: &mut Kmem) -> Result<(), Error> {
        let root = self.root(kmem)?;
        root.unmap(kmem);
        Ok(())
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
