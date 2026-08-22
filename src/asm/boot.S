# qemu -kernel loads the kernel at 0x80000000 and causes each hardware thread to jump there
# virt.ld causes the following code to be placed at 0x80000000

.option norvc
.section .text.init
.global _entry
_entry:
  // the 512kb stack region is divided among 4 hardware threads (128kb each)
  la sp, _stack_addr
  li a0, 0x20000
  // since the stack grows downwards, we set the stack pointer to the very end
  // of the 128kb region
  csrr a1, mhartid
  addi a1, a1, 1
  mul a0, a0, a1
  add sp, sp, a0
  // jump into rust
  call boot

spin:
  j spin
