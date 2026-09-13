//! Monterey compatibility shim for native ScreenCaptureKit recording.
use std::path::Path;
use cua_driver_core::video::{VideoBackend, VideoBackendFactory};
pub struct SckitVideoBackendFactory;
impl VideoBackendFactory for SckitVideoBackendFactory {
    fn start(&self,_output_path:&Path)->anyhow::Result<Box<dyn VideoBackend>>{
        anyhow::bail!("native ScreenCaptureKit video recording is unavailable on the macOS 12.7 compatibility build")
    }
}
