# Assembly-level trap handler.

########################
# CPU INSTRUCTIONS
########################
.section .text

.global asm_trap_vector
# Get here when the cpu is interrupted.
asm_trap_vector:
  wfi
  j asm_trap_vector
