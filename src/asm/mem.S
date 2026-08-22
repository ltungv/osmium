########################
# read-only data
########################
.section .rodata

.global MEM_ADDR
MEM_ADDR: .dword _mem_addr

.global MEM_SIZE
MEM_SIZE: .dword _mem_size

.global TRAMP_ADDR
TRAMP_ADDR: .dword _tramp_addr

.global RODATA_ADDR
RODATA_ADDR: .dword _rodata_addr

.global DATA_ADDR
DATA_ADDR: .dword _data_addr

.global BSS_ADDR
BSS_ADDR: .dword _bss_addr

.global STACK_ADDR
STACK_ADDR: .dword _stack_addr

.global HEAP_ADDR
HEAP_ADDR: .dword _heap_addr

########################
# data
########################
.section .data
