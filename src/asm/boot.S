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

  # Initialize machine's status register:
  # 0b11 << 11: machine's previous protection mode is 3 (MPP=3).
  li t0, (0b11 << 11) 
  csrw mstatus, t0

  # Initialize machine's exception program counter to `kmain`.
  la t1, kinit
  csrw mepc, t1

  # Initialize machine's trap vector base address to `asm_trap_vector`.
  la t2, asm_trap_vector
  csrw mtvec, t2

  # Set the return address to wait infinitely.
  la ra, 2f

  # Use `mret` so `mstatus` is properly updated.
  mret

2:
  # Initialize supervisor's status register:
  # 1 << 8: Supervisor's previous protection mode is 1 (SPP=1 [Supervisor]).
  # 1 << 5: Supervisor's previous interrupt-enable bit is 1 (SPIE=1 [Enabled]).
  # 1 << 1: Supervisor's interrupt-enable bit will be set to 1 after sret.
  li t0, (1 << 8) | (1 << 5) | (1 << 1)
  csrw sstatus, t0

  # Initialize supervisor's exception program counter to `kmain`.
  la t1, kmain
  csrw sepc, t1

  # Initialize supervisor's trap vector base address to `asm_trap_vector`.
  la t2, asm_trap_vector
  csrw stvec, t2

  # Setting machine interrupt delegate register:
  # 1 << 9: External interrupt delegated to supervisor mode
  # 1 << 5: Timer interrupt delegated to supervisor mode
  # 1 << 1: Software interrupt delegated to supervisor mode
  # By default all traps (interrupts or exceptions) automatically
  # cause an elevation to the machine privilege mode (mode 3).
  # When we delegate, we're telling the CPU to only elevate to
  # the supervisor privilege mode (mode 1)
  li t3, (1 << 9) | (1 << 5) | (1 << 1)
  csrw mideleg, t3

  # Setting supervisor interrupt enable register:
  # 1 << 9: Supervisor external interrupt enable (SEIE=1 [Enabled])
  # 1 << 5: Supervisor timer interrupt enable (STIE=1 [Enabled])
  # 1 << 1: Supervisor software interrupt enable (SSIE=1 [Enabled])
  csrw sie, t3

  # kinit() is required to return back the SATP value (including MODE) via a0
  csrw satp, a0

  # Force the CPU to take our SATP register.
  # To be efficient, if the address space identifier (ASID) portion of SATP is already
  # in cache, it will just grab whatever's in cache. However, that means if we've updated
  # it in memory, it will be the old table. So, sfence.vma will ensure that the MMU always
  # grabs a fresh copy of the SATP register and associated tables.
  sfence.vma

  # Use `mret` so `mstatus` is properly updated.
  sret

3:
  # Parked hardware threads jump here and wait indefinitely.

4:
  # Repeating wait for interrupts.
  wfi
  j 4b
