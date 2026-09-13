fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") { return; }
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    let sdk_root=std::env::var("SDKROOT").unwrap_or_else(|_| std::process::Command::new("xcrun").args(["--sdk","macosx","--show-sdk-path"]).output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).unwrap_or_default());
    let sdk_root=sdk_root.trim();
    if !sdk_root.is_empty() { println!("cargo:rustc-link-search=framework={sdk_root}/System/Library/Frameworks"); }
}
