fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");

    // Do not add SDK/usr/lib/system on Monterey: direct libdispatch linking is
    // rejected by the Monterey linker. The wrapper maps -ldispatch to System.
    let sdk_root = std::env::var("SDKROOT").unwrap_or_else(|_| {
        let out = std::process::Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        out.trim().to_owned()
    });
    if !sdk_root.is_empty() {
        println!("cargo:rustc-link-search=framework={sdk_root}/System/Library/Frameworks");
    }
}
