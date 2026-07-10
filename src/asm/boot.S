# bootloader for osmium.

.option norvc
.section .data
.section .text.init

.global _start
_start:
  # read the hardware thread id from the `mhartid` register into the `t0` register. park the thread
  csrr t0, mhartid
  bnez t0, 3f

  # set supervisor address translation and protection register to 0.
  csrw satp, zero

  # disable linker instruction relaxation for the `la` instruction, disallowing the assembler from
  # assuming `gp` is # already initialized. this causes `gp` to be calculated from `pc`.
.option push
.option norelax
  la gp, _global_pointer
.option pop

  # initialize all bytes in the bss section to zero.
  la a0, _bss_start
  la a1, _bss_end
  bgeu a0, a1, 2f

1:
  sd zero, (a0)
  addi a0, a0, 8
  bltu a0, a1, 1b

2:
  # initialize the stack pointer.
  la sp, _stack_end

  # initialize `mstatus` register:
	# 0b11 << 11: machine's previous protection mode is 3 (mpp=3).
	# 1 << 7: machine's previous interrupt-enable bit is 1 (mpie=1).
	# 1 << 3: machine's interrupt-enable bit is 1 (mie=1).
	li t0, (0b11 << 11) | (1 << 7) 
  #| (1 << 3)
  csrw mstatus, t0

  # initialize machine's exception program counter (mepc) to `kmain`.
	la t1, kmain
	csrw mepc, t1

  # initialize machine's trap vector base address to `asm_trap_vector`.
  la t2, asm_trap_vector
	csrw mtvec, t2

  # initialize machine's interrupt-enable bits (mie)
	# 1 << 11: machine's external interrupt-enable bit is 1 (meie=1).
	# 1 << 7: machine's timer interrupt-enable bit is 1 (mtie=1).
  # 1 << 3: machine's m-mode software interrupt-enable bit is 1 (msie=1).
  # li t3, (1 << 11) | (1 << 7) | (1 << 3)
	# csrw mie, t3

  # set the return address to infinitely wait for interrupts.
	la ra, 4f
  # use `mret` so `mstatus` is properly updated.
	mret

3:
  # parked hardware threads jump here and wait indefinitely for interrupts.

4:
  # wait for interrupt
  wfi
  # jump back to the beginning of the loop
  j 3b

