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
# Set `mstatus` to machine mode, giving us access to all registers and instructions.
# Ignore interrupts when running set up code.
    li t0, (1 << 12) | (1 << 11)
    csrw mstatus, t0
# Set machine's exception program counter (mepc) to `kmain`.
    la t1, kmain
    csrw mepc, t1
# Set machine's trap vector base address to `asm_trap_vector`.
    la t2, asm_trap_vector
    csrw mtvec, t2
# Set the return address to wait indefinitely for interrupts.
    la ra, 4f
# Use `mret` to get `mstatus` properly updated.
    mret
3:
# Parked harts go here. We need to set these to only awaken if it receives
# a software interrupt, which we're going to call the SIPI (Software
# Intra-Processor Interrupt). We only use these to run user-space programs,
# although this may change.
4:
    wfi
    j 4b