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
  # read the hardware thread id from the `mhartid` register into the `t0` register.
  # park any thread whose id is not zero. a thread with id zero always exists.
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

  # Initialize all bytes in the bss section to zero.
  la a0, _bss_start
  la a1, _bss_end
  bgeu a0, a1, 2f

1:
  sd zero, (a0)
  addi a0, a0, 8
  bltu a0, a1, 1b

2:
  # Initialize the stack pointer.
  la sp, _stack_end

  # Initialize `mstatus` register:
  # 0b11 << 11: machine's previous protection mode is 3 (MPP=3).
  # 1 << 7: machine's previous interrupt-enable bit is 1 (MPIE=1).
  # 1 << 3: machine's interrupt-enable bit is 1 (MIE=1).
  li t0, (0b11 << 11) | (1 << 7) | (1 << 3)
  csrw mstatus, t0

  # Initialize machine's exception program counter to `kmain`.
  la t1, kmain
  csrw mepc, t1

  # Initialize machine's trap vector base address to `asm_trap_vector`.
  la t2, asm_trap_vector
  csrw mtvec, t2

  # Initialize machine's interrupt-enable bits:
  # 1 << 11: machine's external interrupt-enable bit is 1 (MEIE=1).
  # 1 << 7: machine's timer interrupt-enable bit is 1 (MTIE=1).
  # 1 << 3: machine's m-mode software interrupt-enable bit is 1 (MSIE=1).

  # TODO: Uncomment this section to enable interrupt. This is disable so we can run the kernel without
  # having implemented an interrupt handler.
  # li t3, (1 << 11) | (1 << 7) | (1 << 3)
  # csrw mie, t3

  # Set the return address to wait infinitely.
  la ra, 4f

  # Use `mret` so `mstatus` is properly updated.
  mret

3:
  # Parked hardware threads jump here and wait indefinitely.

4:
  # Repeating wait for interrupts.
  wfi
  j 3b
