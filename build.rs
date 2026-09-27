const ASM_FILES: [&str; 2] = ["src/kern/asm/trampoline.S", "src/kern/asm/trap.S"];

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
