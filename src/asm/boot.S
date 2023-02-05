# Disable generation of compressed instructions.
.option norvc
# Defined a .data section.
.section .data
# Defined a .text.init section.
.section .text.init
# Execution starts here
.global _start

_start:
# Choose the default hardware threads (HART) for bootstrapping,
# other HARTs will wait for an IPI.
    csrr t0, mhartid
    bnez t0, 3f
# Supervisor Address Translation and Protection (satp) should already be 0.
    csrw satp, zero
# Disable linked instructions relaxation for the `la` instruction below.
.option push
.option norelax
# Assembler can't assume that `gp` is already initialized causing `gp`
# to be recalculated from `pc`.
    la gp, _global_pointer
.option pop
# Initialize all bytes in BSS section to 0.
    la a0, _bss_start
    la a1, _bss_end
# Don't need to loop if there's no data in BSS.
    bgeu a0, a1, 2f
1:
# Loop through memory addresses in range between `_bss_start` and `_bss_end`,
# and set their value to 0
    sd zero, (a0)
    addi a0, a0, 8
    bltu a0, a1, 1b
2:
# Set the stack.
    la sp, _stack_end
# Set `mstatus`.
# + (1 << 12) | (1 << 11) : Previous protection mode is 3 (M-mode)
    li t0, (1 << 12) | (1 << 11)
    csrw mstatus, t0
# Set machine's exception program counter (mepc) to `kinit`.
    la t1, kinit
    csrw mepc, t1
# Set machine's trap vector base address to `asm_trap_vector`.
    la t2, asm_trap_vector
    csrw mtvec, t2
# Set the return address to get into supervisor mode
    la ra, 2f
# Use `mret` to get `mstatus` properly updated.
    mret
2:
# Set `sstatus`
# + (1 << 8) : Previous protection mode is 1 (S-mode)
# + (1 << 5) : Previous interrupt-enabled bit is 1
# + (1 << 1) : Interrupt-enabled bit is 1
    li t0, (1 << 8) | (1 << 5) | (1 << 1)
    csrw sstatus, t0
# Set supervisor's exception program counter (sepc) to `kmain`.
    la t1, kmain
    csrw sepc, t1
# # Set the same bits for both the machine interrupt delegate register (mideleg)
# # and supervisor interrupt enabled register (sie).  These types of traps
# # (interrupts / exceptions) are delegated to S-mode:
# # + 1 << 1 : Software interrupt
# # + 1 << 5 : Timer interrupt
# # + 1 << 9 : External interrupt
    li t2, (1 << 9) | (1 << 5) | (1 << 1)
    csrw mideleg, t2
    csrw sie, t2
# Set supervisor's trap vector base address to `asm_trap_vector`.
    la t3, asm_trap_vector
    csrw stvec, t3
# kinit() is required to return back the SATP value (including MODE) via a0
    csrw satp, a0
# Force the CPU to take our SATP register.
# To be efficient, if the address space identifier (ASID) portion of SATP is already
# in cache, it will just grab whatever's in cache. However, that means if we've updated
# it in memory, it will be the old table. So, sfence.vma will ensure that the MMU always
# grabs a fresh copy of the SATP register and associated tables.
    sfence.vma
# sret will put us in supervisor mode and re-enable interrupts.
    sret
3:
# Parked harts go here. We need to set these to only awaken if it receives
# a software interrupt, which we're going to call the SIPI (Software
# Intra-Processor Interrupt). We only use these to run user-space programs,
# although this may change.
4:
    wfi
    j 4b