# Assembly-level trap handler.

########################
# CPU INSTRUCTIONS
########################
.section .text

.global asm_trap_vector
asm_trap_vector:
  # Get here when the cpu is interrupted.
  mret

