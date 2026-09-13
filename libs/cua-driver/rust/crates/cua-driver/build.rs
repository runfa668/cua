// Monterey build avoids the ScreenCaptureKit Swift bridge, so no Swift runtime rpaths are required.
fn main() {
    #[cfg(target_os = "windows")]
    { embed_resource::compile("cua-driver.rc", embed_resource::NONE); }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") { return; }
    let sdk_root = std::env::var("SDKROOT").unwrap_or_else(|_| std::process::Command::new("xcrun").args(["--sdk","macosx","--show-sdk-path"]).output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).unwrap_or_default());
    let sdk_root=sdk_root.trim();
    if !sdk_root.is_empty() { println!("cargo:rustc-link-search=framework={sdk_root}/System/Library/Frameworks"); }
}
