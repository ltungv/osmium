/* linker script for risc-v qemu "virt" machine */

/* set risc-v as the architecture */
OUTPUT_ARCH(riscv)

/* set _entry as the entry point */
ENTRY(_entry)

/* a single 128mb "ram" section that is:
 * - 'w': writeable
 * - 'x': executable
 * - 'a': allocatable
 * - '!r': not read-only
 * - '!i': not initialized
 */
MEMORY {
	ram (wxa!ri) : ORIGIN = 0x80000000, LENGTH = 128M
}

/* 3 programs header that will be loaded directly from file into memory:
 * - text: cpu instructions (executable sections)
 * - data: global, initialized variables
 * - bss: global, uninitialized variables (all will be set to 0 by boot.s)
 */
PHDRS {
	text PT_LOAD;
	data PT_LOAD;
	bss PT_LOAD;
}

/* the memory is organized based on which section it is in. in rust, we have:
 * - cpu instructions stored in `text`
 * - global constants stored in `rodata`
 * - global initialized variables stored in `data`
 * - global uninitialized variables stored in `bss`
 */
SECTIONS {
	PROVIDE(_mem_size = LENGTH(ram));
  PROVIDE(_mem_addr = ORIGIN(ram));

	.text : {
		*(.text.init) *(.text .text.*)
    . = ALIGN(4096);
    PROVIDE(_tramp_addr = .);
    *(tramp)
	} >ram AT>ram :text

	.rodata : {
    . = ALIGN(4096);
		PROVIDE(_rodata_addr = .);
    ASSERT(_rodata_addr - _tramp_addr == 0x1000, "error: trampoline is larger than one page");
		*(.srodata .srodata.*) *(.rodata .rodata.*)
	} >ram AT>ram :text

  .data : {
    . = ALIGN(4096);
		PROVIDE(_data_addr = .);
		*(.sdata .sdata.*) *(.data .data.*)
	} >ram AT>ram :data

	.bss : {
		. = ALIGN(4096);
		PROVIDE(_bss_addr = .);
		*(.sbss .sbss.*) *(.bss .bss.*)
	} >ram AT>ram :bss

  . = ALIGN(4096);
  PROVIDE(_stack_addr = .);
	PROVIDE(_heap_addr = _stack_addr + 0x80000);
}
