// — PatchBay: injects version + build number into the kernel binary at compile time.
// NT-style versioning: 0.1.0.147 = major.minor.patch.build

fn main() {
    // Forward OXIDE_BUILD_NUMBER and OXIDE_VERSION_STRING to the crate via cargo:rustc-env
    // These are set by mk/kernel.mk from the build/build-number file
    let build = std::env::var("OXIDE_BUILD_NUMBER").unwrap_or_else(|_| "0".to_string());
    let version = std::env::var("OXIDE_VERSION_STRING").unwrap_or_else(|_| "0.1.0".to_string());

    println!("cargo:rustc-env=OXIDE_BUILD_NUMBER={build}");
    println!("cargo:rustc-env=OXIDE_VERSION_STRING={version}");

    // Re-run if env changes (build number increments)
    println!("cargo:rerun-if-env-changed=OXIDE_BUILD_NUMBER");
    println!("cargo:rerun-if-env-changed=OXIDE_VERSION_STRING");
}
