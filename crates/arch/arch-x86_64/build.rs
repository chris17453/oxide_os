fn main() {
    // Compile AP boot trampoline assembly
    cc::Build::new()
        .file("src/ap_boot.s")
        .compile("ap_boot");

    // Re-run if assembly file changes
    println!("cargo:rerun-if-changed=src/ap_boot.s");
}
