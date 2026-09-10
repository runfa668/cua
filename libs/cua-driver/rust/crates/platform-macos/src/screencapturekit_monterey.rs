//! Minimal ScreenCaptureKit compatibility surface for the Monterey branch.
//!
//! Upstream check_permissions uses `SCShareableContent::get()` only as a live
//! screen-recording capability probe. The real `screencapturekit` crate pulls
//! in a Swift bridge requiring a newer toolchain, so this module preserves the
//! tiny API surface needed by that call and delegates permission state to the
//! native CoreGraphics/TCC preflight implementation already used by cua-driver.

pub mod prelude {
    /// Monterey compatibility stand-in for ScreenCaptureKit shareable content.
    pub struct SCShareableContent {
        capturable: bool,
    }

    pub struct SCDisplay;

    impl SCShareableContent {
        pub fn get() -> Result<Self, &'static str> {
            Ok(Self {
                capturable: crate::permissions::status::screen_recording_granted(),
            })
        }

        pub fn displays(&self) -> Vec<SCDisplay> {
            if self.capturable {
                vec![SCDisplay]
            } else {
                Vec::new()
            }
        }
    }
}
