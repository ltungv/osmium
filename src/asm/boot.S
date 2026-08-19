# Bootloader for osmium.

.option norvc

########################
# DATA
########################
.section .data

########################
# CPU INSTRUCTIONS
########################
.section .text.init

.global _start
_start:
  # Read the hardware thread id from `mhartid` register into `t0` register.
  # Park any thread whose id is not zero.
  csrr t0, mhartid
  bnez t0, 3f
  # Set supervisor address translation and protection register to 0.
  csrw satp, zero
  # Disable linker instruction relaxation for the `la` instruction, disallowing
  # the assembler from assuming `gp` is already initialized.  This causes `gp`
  # to be calculated from `pc`.
.option push
.option norelax
  la gp, _global_pointer
.option pop
  # Set all bytes in the bss section to zero.
  la a0, _bss_start
  la a1, _bss_end
  bgeu a0, a1, 2f

1:
  sd zero, (a0)
  addi a0, a0, 8
  bltu a0, a1, 1b

2:
  # Set the stack pointer.
  la sp, _stack_end
  # Set machine status register:
  # 0b01 << 13: Floating-point unit state is 1 (FS=1 [Initial]).
  # 0b11 << 11: Machine previous privilege mode is 3 (MPP=3 [Machine]).
  li t0, (0b01 << 13) | (0b11 << 11)
  csrw mstatus, t0
  # Set machine exception program counter to `kinit`.
  la t0, kinit
  csrw mepc, t0
  # Do not allow interrupts while running kinit.
  csrw	mie, zero
  # Set the return address to get into supervisor mode.
  la ra, 2f
  # Use `mret` so `mstatus` is properly updated.
  mret

2:
  # Set machine status register:
  # 0b01 << 11: Machine previous privilege mode is 1 (MPP=1 [Supervisor]).
  # 1 << 7: Machine previous interrupt enable bit is 1.
  # 1 << 5: Supervisor previous interrupt enable bit is 1.
  li t0, (0b01 << 11) | (1 << 7) | (1 << 5)
  csrw mstatus, t0
  # Set machine exception program counter to `kmain`.
  la t0, kmain
  csrw mepc, t0
  # Set machine trap vector base address to `_mtrap`.
  la t0, _mtrap
  csrw mtvec, t0
  # Configure physical memory protection (PMP).
  li t0, -1
  csrw pmpaddr0, t0
  # R (bit 0) = 1
  # W (bit 1) = 1
  # X (bit 2) = 1
  # A (bits 3:4) = 3 (NAPOT - Naturally Aligned Power-Of-Two)
  li t0, 0x1f
  csrw pmpcfg0, t0
  # Clear the translation lookaside buffer (TLB).
  sfence.vma
  # Drop to supervisor mode
  # + sets the program counter to `mepc`,
  # + sets the privilege mode to `mstatus.MPP`,
  # + sets `mstatus.MIE` to `mstatus.MPIE` (which we set to 0).
  mret

3:
  # Parked hardware threads jump here.

4:
  # Wait indefinitely for interrupts.
  wfi
  j 4b
