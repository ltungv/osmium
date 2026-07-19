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
  # Read the hardware thread id from the `mhartid` register into the `t0` register.
  # Park any thread whose id is not zero. A thread with id zero always exists.
  csrr t0, mhartid
  bnez t0, 3f
  # Set supervisor address translation and protection register to 0.
  csrw satp, zero
  # Disable linker instruction relaxation for the `la` instruction, disallowing the assembler from
  # assuming `gp` is already initialized. This causes `gp` to be calculated from `pc`.
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
  # Set machine's status register:
  # 0b11 << 11: Machine's previous protection mode is 3 (MPP=3 [Machine]).
  li t0, (0b11 << 11) 
  csrw mstatus, t0
  # Set machine's exception program counter to `kinit`.
  la t0, kinit
  csrw mepc, t0
  # Do not allow interrupts while running kinit
  csrw	mie, zero
  # Set the return address to get into supervisor mode 
  la ra, 2f
  # Use `mret` so `mstatus` is properly updated.
  mret

2:
  # Configure mstatus (Machine Status Register)
  # MPP (bits 11:12) = 01 (Set Previous Privilege to Supervisor mode)
  # MPIE (bit 7) = 0 (Previous Interrupt Enable is 0. This ensures sstatus.SIE = 0 after mret)
  # MIE (bit 3) = 0 (Current interrupts are disabled)
  li t0, (1 << 11)
  csrw mstatus, t0
  # Set the Supervisor mode entry point
  # 'mepc' tells the CPU where to jump when 'mret' is executed
  la t0, kmain
  csrw mepc, t0
  # Delegate traps to Supervisor mode
  # Even though interrupts are disabled, we want synchronous exceptions
  # (like page faults or illegal instructions) to be handled by S-mode, not M-mode.
  li t0, 0xffff
  csrw medeleg, t0   # Delegate exceptions to S-mode
  csrw mideleg, t0   # Delegate interrupts to S-mode (has no effect right now since they are disabled)
  # Explicitly disable all specific interrupts
  csrw mie, zero
  csrw sie, zero
  # Set supervisor's trap vector base address to `asm_trap_vector`.
  la t0, asm_trap_vector
  csrw stvec, t0
  # Configure Physical Memory Protection (PMP)
  # By default, S-mode has NO memory access. We must grant it access here.
  li t0, -1
  csrw pmpaddr0, t0
  # pmpcfg0 bits:
  # R (bit 0) = 1, W (bit 1) = 1, X (bit 2) = 1
  # A (bits 3:4) = 3 (NAPOT mode - Naturally Aligned Power-Of-Two)
  # 0b00011111 = 0x1F
  li t0, 0x1F
  csrw pmpcfg0, t0

  csrw satp, a0
  sfence.vma

  # Drop to Supervisor mode!
  # This instruction sets the program counter to 'mepc',
  # sets the privilege mode to 'mstatus.MPP',
  # and sets the global interrupt enable to 'mstatus.MPIE' (which we set to 0).
  mret

3:
  # Parked hardware threads jump here and wait indefinitely.

4:
  # Repeating wait for interrupts.
  wfi
  j 4b
