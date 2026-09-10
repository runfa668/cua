// Platform-specific linker setup for the SDK cdylib.
//
// Monterey compatibility deliberately avoids the ScreenCaptureKit Swift
// bridge. Do not add Swift runtime rpaths here: the SDK should link using the
// system libraries/frameworks available in the Monterey SDK.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libcua_driver_sdk.dylib");
    }
}
