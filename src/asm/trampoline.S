# low-level code to handle traps from user space into the kernel, and returns
# from the kernel back to the user
#
# the kernel maps the page holding this code at the same virtual address
# (trampoline) in user and kernel space so that it continues to work when it
# switches page tables
#
# `kernel.ld` causes this code to start at a page boundary

.section tramp
.global trampoline
trampoline:

.align 4
.global usertrap
uservec:
  j uservec

