const ASM_FILES: [&str; 4] = [
    "src/kernel/asm/boot.S",
    "src/kernel/asm/mem.S",
    "src/kernel/asm/trampoline.S",
    "src/kernel/asm/trap.S",
];

fn main() {
    let mut build = cc::Build::new();
    build.flags([
        "-static",
        "-nostdlib",
        "-ffreestanding",
        "-fno-rtti",
        "-fno-exceptions",
        "-march=rv64gc",
        "-mabi=lp64d",
        "-c",
        "-march=rv64gc",
        "-mabi=lp64d",
    ]);
    for asm in ASM_FILES {
        println!("cargo:rerun-if-changed={asm}");
        build.file(asm);
    }
    build.compile("asm");
}
