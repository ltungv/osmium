# Assembly-level trap handler.

.option norvc
.altmacro

# Number of general-purpose registers.
.set NUM_GP_REGS, 32

# Size of a register in bytes.
.set REG_SIZE, 8

# Load the i-th general-purpose register from a scratch memory region starting at `basereg`.
.macro load_gp i, basereg
  ld x\i, ((\i)*REG_SIZE)(\basereg)
.endm

# Save the i-th general-purpose register into a scratch memory region starting at `basereg`.
.macro save_gp i, basereg
  sd x\i, ((\i)*REG_SIZE)(\basereg)
.endm

# Load the i-th floating-point register from a scratch memory region starting at `basereg`.
.macro load_fp i, basereg
  fld f\i, ((NUM_GP_REGS+(\i))*REG_SIZE)(\basereg)
.endm

# Save the i-th floating-point register into a scratch memory region starting at `basereg`.
.macro save_fp i, basereg
  fsd f\i, ((NUM_GP_REGS+(\i))*REG_SIZE)(\basereg)
.endm

########################
# CPU INSTRUCTIONS
########################
.section .text

.global _mtrap
.align 4
_mtrap:
  # Swap the value of `mscratch` and `t6`, so `t6` holds the address to the
  # context memory region (load/save instructions don't work with CSRs).
  # We use `t6` as the temporary register because it's the last register,
  # i.e., the `x31` register.
  csrrw t6, mscratch, t6
  # Save `x0-x30` into the context memory region.
  .set i, 0
  .rept 31
    save_gp %i, t6
    .set i, i+1
  .endr
  # Copy the context memory region address from `t6` to `t5`.
  mv t5, t6
  # Read the previous value of `t6` in `mscratch` back into `t6`.
  csrr t6, mscratch
  # Save `t6` into the context memory region.
  save_gp 31, t5
  # Write the context memory region address back into `mscratch`.
  csrw mscratch, t5
  # Extract FS from `mstatus[14:13]` into `t0`.
  csrr t1, mstatus
  srli t0, t1, 13
  andi t0, t0, 0b11
  # Check FS and skip saving floating-point registers when they're not dirty.
  li t1, 3
  bne t0, t1, 1f
  # Save `f0-f31` into the context memory region.
  .set i, 0
  .rept 32
    save_fp %i, t5
    .set i, i+1
  .endr

1:
  # Context memory layout:
  # [000] 32 gp regs
  # [256] 32 fp regs
  # [512] SATP register
  # [520] Trap stack
  # [528] CPU HARTID
  csrr a0, mepc
  csrr a1, mtval
  csrr a2, mcause
  csrr a3, mhartid
  csrr a4, mstatus
  csrr a5, mscratch
  sd a0, 520(t5)
  la sp, _stack_end
  call mtrap
  # Set machine exception program counter to the address returned by `mtrap`.
  csrw mepc, a0
  # Read the context memory region address into `t6`.
  csrr t6, mscratch
  # Extract FS from `mstatus[14:13]` into `t0`.
  csrr t1, mstatus
  srli t0, t1, 13
  andi t0, t0, 0b11
  # Check FS and skip loading floating-point registers when they're not dirty.
  li t1, 3
  bne t0, t1, 1f
  # Load `f0-f31` from the context memory region.
  .set i, 0
  .rept 32
    load_fp %i, t6
    .set i, i+1
  .endr

1:
  # Load `x1-x31` from the context memory region.
  .set i, 1
  .rept 31
    load_gp %i, t6
    .set  i, i+1
  .endr
  mret