# assembly-level trap handler.

########################
# CPU instructions
########################
.section .text

.global asm_trap_vector
asm_trap_vector:
  # get here when the cpu is interrupted.
  mret

