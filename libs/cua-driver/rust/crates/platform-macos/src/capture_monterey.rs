//! macOS Monterey-compatible screenshot backend.
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::process::Command;
struct SecureCapturePath { directory: std::path::PathBuf, file: std::path::PathBuf }
impl SecureCapturePath { fn new(file_name:&str)->anyhow::Result<Self>{ use std::os::unix::fs::DirBuilderExt; let directory=std::env::temp_dir().join(format!("cua-driver-rs-capture-{}-{}",std::process::id(),uuid::Uuid::new_v4())); std::fs::DirBuilder::new().mode(0o700).create(&directory)?; let file=directory.join(file_name); Ok(Self{directory,file}) } }
impl Drop for SecureCapturePath { fn drop(&mut self){ let _=std::fs::remove_file(&self.file); let _=std::fs::remove_dir(&self.directory); } }
pub fn screenshot_window_bytes(window_id:u32)->anyhow::Result<Vec<u8>>{ let capture=SecureCapturePath::new("window.png")?; let path=capture.file.to_string_lossy().into_owned(); let output=Command::new("/usr/sbin/screencapture").args(["-l",&window_id.to_string(),"-x","-o",&path]).output()?; if !output.status.success(){ anyhow::bail!("screencapture failed for window {window_id}: {}",String::from_utf8_lossy(&output.stderr)); } let bytes=std::fs::read(&capture.file)?; if bytes.is_empty(){anyhow::bail!("screencapture produced empty output")}; Ok(bytes) }
pub fn screenshot_window(window_id:u32)->anyhow::Result<(String,u32,u32)>{ let bytes=screenshot_window_bytes(window_id)?; let (w,h)=png_dimensions(&bytes)?; Ok((BASE64.encode(&bytes),w,h)) }
pub fn screenshot_display_bytes()->anyhow::Result<Vec<u8>>{ let capture=SecureCapturePath::new("display.png")?; let path=capture.file.to_string_lossy().into_owned(); let output=Command::new("/usr/sbin/screencapture").args(["-x",&path]).output()?; if !output.status.success(){anyhow::bail!("screencapture failed: {}",String::from_utf8_lossy(&output.stderr));} let bytes=std::fs::read(&capture.file)?; if bytes.is_empty(){anyhow::bail!("screencapture produced empty output")}; Ok(bytes) }
pub fn screenshot_display()->anyhow::Result<(String,u32,u32)>{ let bytes=screenshot_display_bytes()?; let (w,h)=png_dimensions(&bytes)?; Ok((BASE64.encode(&bytes),w,h)) }
pub fn png_bytes_to_jpeg(png_bytes:&[u8],quality:u8)->anyhow::Result<Vec<u8>>{cua_driver_core::image_utils::png_bytes_to_jpeg(png_bytes,quality)}
pub fn resize_png_if_needed(png_bytes:&[u8],max_dim:u32)->anyhow::Result<Vec<u8>>{cua_driver_core::image_utils::resize_png_if_needed(png_bytes,max_dim)}
pub fn write_crosshair_png(png_bytes:&[u8],cx:f64,cy:f64,path:&str)->anyhow::Result<()>{cua_driver_core::image_utils::write_crosshair_png(png_bytes,cx,cy,path)}
pub fn crosshair_png_bytes(png_bytes:&[u8],cx:f64,cy:f64)->anyhow::Result<Vec<u8>>{cua_driver_core::image_utils::crosshair_png_bytes(png_bytes,cx,cy)}
pub fn png_dimensions(data:&[u8])->anyhow::Result<(u32,u32)>{cua_driver_core::image_utils::png_dimensions(data)}
