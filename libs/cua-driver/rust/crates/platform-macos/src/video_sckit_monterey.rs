//! Monterey compatibility shim for the native ScreenCaptureKit recorder.
//!
//! SCRecordingOutput requires macOS 15. Keep the factory/type surface so the
//! rest of cua-driver can compile, but fail cleanly when native recording is
//! requested on the Monterey compatibility branch.

use std::path::Path;
use cua_driver_core::video::{VideoBackend, VideoBackendFactory};

pub struct SckitVideoBackendFactory;

impl VideoBackendFactory for SckitVideoBackendFactory {
    fn start(&self, _output_path: &Path) -> anyhow::Result<Box<dyn VideoBackend>> {
        anyhow::bail!(
            "native ScreenCaptureKit video recording is unavailable on the macOS 12.7 compatibility build"
        )
    }
}
