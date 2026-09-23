.global kerneltrap
.global kernelvec
.align 4
kernelvec:
  # make space in the stack to save registers
  addi sp, sp, -128

  # backup caller-saved registers.
  # return address
  sd ra, 0(sp)
  # global pointer
  sd gp, 8(sp)
  # temporaries
  sd t0, 16(sp)
  sd t1, 24(sp)
  sd t2, 32(sp)
  # arguments
  sd a0, 40(sp)
  sd a1, 48(sp)
  sd a2, 56(sp)
  sd a3, 64(sp)
  sd a4, 72(sp)
  sd a5, 80(sp)
  sd a6, 88(sp)
  sd a7, 96(sp)
  # temporaries
  sd t3, 104(sp)
  sd t4, 112(sp)
  sd t5, 120(sp)
  sd t6, 128(sp)

  # handle trap
  call kerneltrap

  # restore registers
  # return address
  ld ra, 0(sp)
  # global pointer
  ld gp, 8(sp)
  # temporaries
  ld t0, 16(sp)
  ld t1, 24(sp)
  ld t2, 32(sp)
  # arguments
  ld a0, 40(sp)
  ld a1, 48(sp)
  ld a2, 56(sp)
  ld a3, 64(sp)
  ld a4, 72(sp)
  ld a5, 80(sp)
  ld a6, 88(sp)
  ld a7, 96(sp)
  # temporaries
  ld t3, 104(sp)
  ld t4, 112(sp)
  ld t5, 120(sp)
  ld t6, 128(sp)

  addi sp, sp, 128
  sret
