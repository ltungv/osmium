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
  # Set the stack pointer.
  la sp, _stack_end
  # Set all bytes in the bss section to zero.
  la a0, _bss_start
  la a1, _bss_end
  bgeu a0, a1, 2f

1:
  sd zero, (a0)
  addi a0, a0, 8
  bltu a0, a1, 1b

2:
  # Set machine's status register:
  # 0b11 << 11: Machine's previous protection mode is 3 (MPP=3 [Machine]).
  li t0, (0b11 << 11) 
  csrw mstatus, t0
  # Do not allow interrupts while running kinit
  csrw	mie, zero
  # Set machine's exception program counter to `kinit`.
  la t0, kinit
  csrw mepc, t0
  # Set the return address to get into supervisor mode 
  la ra, 2f
  # Use `mret` so `mstatus` is properly updated.
  mret

2:
  # Set machine interrupt delegate register:
  # 1 << 9: External interrupt delegated to supervisor mode
  # 1 << 5: Timer interrupt delegated to supervisor mode
  # 1 << 1: Software interrupt delegated to supervisor mode
  li t0, (1 << 9) | (1 << 5) | (1 << 1)
  csrw mideleg, t0
  # Set supervisor's status register:
  # 1 << 8: Supervisor's previous protection mode is 1 (SPP=1 [Supervisor]).
  # 1 << 5: Supervisor's previous interrupt-enable bit is 1 (SPIE=1 [Enabled]).
  # 1 << 1: Supervisor's interrupt-enable bit will be set to 1 after sret.
  li t0, (1 << 8) | (1 << 5) | (1 << 1)
  csrw sstatus, t0
  # Set supervisor interrupt enable register:
  # 1 << 9: Supervisor external interrupt enable (SEIE=1 [Enabled])
  # 1 << 5: Supervisor timer interrupt enable (STIE=1 [Enabled])
  # 1 << 1: Supervisor software interrupt enable (SSIE=1 [Enabled])
  li t0, (1 << 9) | (1 << 5) | (1 << 1)
  csrw sie, t0
  # Set supervisor's exception program counter to `kmain`.
  la t0, kmain
  csrw sepc, t0
  # Set supervisor's trap vector base address to `asm_trap_vector`.
  la t0, asm_trap_vector
  csrw stvec, t0
  # kinit() is required to return back the SATP value (including MODE) via a0
  csrw satp, a0
  # Force the CPU to take our SATP register.
  sfence.vma
  # Use `mret` so `mstatus` is properly updated.
  sret

3:
  # Parked hardware threads jump here and wait indefinitely.

4:
  # Repeating wait for interrupts.
  wfi
  j 4b
