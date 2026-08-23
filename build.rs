fn main() {
    println!("cargo:rustc-link-arg=-Wl,--undefined=trampoline");
    println!("cargo:rustc-link-arg=-Tvirt.ld");

    cc::Build::new()
        .flags([
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
        ])
        .file("src/asm/boot.S")
        .file("src/asm/mem.S")
        .file("src/asm/trampoline.S")
        .compile("asm");

    println!("cargo:rerun-if-changed=virt.ld");
    println!("cargo:rerun-if-changed=src/asm/boot.S");
    println!("cargo:rerun-if-changed=src/asm/mem.S");
    println!("cargo:rerun-if-changed=src/asm/trampoline.S");
}
