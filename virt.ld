/* set risc-v as the architecture */
OUTPUT_ARCH(riscv)

/* set _entry as the entry point */
ENTRY(_entry)

/* define a single 128mb "ram" section */
MEMORY {
	ram : ORIGIN = 0x80000000, LENGTH = 128M
}

/* define 3 programs headers:
 * - text: executable sections
 * - rodata: read-only sections
 * - data: global, initialized variables
 * - bss: global, uninitialized variables
 */
PHDRS {
	text PT_LOAD;
	rodata PT_LOAD;
	data PT_LOAD;
	bss PT_LOAD;
}

SECTIONS {
	PROVIDE(_mem_size = LENGTH(ram));
  PROVIDE(_mem_addr = ORIGIN(ram));

	.text : {
		*(.text.init) *(.text .text.*)
    . = ALIGN(4096);
    PROVIDE(_tramp_addr = .);
    KEEP(*(.tramp))
	} >ram :text

	.rodata : {
    . = ALIGN(4096);
		PROVIDE(_rodata_addr = .);
		ASSERT(_rodata_addr - _tramp_addr == 0x1000, "trampoline is not a single page");
		*(.srodata .srodata.*) *(.rodata .rodata.*)
	} >ram :rodata

  .data : {
    . = ALIGN(4096);
		PROVIDE(_data_addr = .);
		*(.sdata .sdata.*) *(.data .data.*)
	} >ram :data

	.bss : {
		. = ALIGN(4096);
		PROVIDE(_bss_addr = .);
		*(.sbss .sbss.*) *(.bss .bss.*)
	} >ram :bss

  . = ALIGN(4096);
  PROVIDE(_stack_addr = .);
	PROVIDE(_heap_addr = _stack_addr + 0x80000);
}
