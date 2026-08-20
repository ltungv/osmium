# qemu -kernel loads the kernel at 0x80000000 and causes each hardware thread to jump there.
# virt.ld causes the following code to be placed at 0x80000000.

.option norvc
.section .text.init
.global _entry
_entry:
  // The 512KB stack region is divided among 4 hardware threads (128KB each).
  la sp, _stack_start
  li a0, 0x20000
  // Since the stack grows downwards, we set the stack pointer to the very end
  // of the 128KB region.
  csrr a1, mhartid
  addi a1, a1, 1
  mul a0, a0, a1
  add sp, sp, a0
  // Jump into Rust.
  call boot

spin:
  wfi
  j spin
