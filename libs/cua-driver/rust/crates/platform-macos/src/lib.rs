//! macOS platform backend for cua-driver-rs.
//!
//! Monterey compatibility branch: AX/input remain native; still screenshots
//! use the system screencapture utility to avoid macOS 14+ screenshot APIs.

#[cfg(target_os = "macos")]
pub mod apps;
#[cfg(target_os = "macos")]
pub mod ax;
#[cfg(target_os = "macos")]
mod background_mutation;
#[cfg(target_os = "macos")]
pub mod browser;
#[cfg(target_os = "macos")]
#[path = "capture_monterey.rs"]
pub mod capture;
#[cfg(target_os = "macos")]
pub mod cursor;
#[cfg(target_os = "macos")]
pub mod focus_guard;
#[cfg(target_os = "macos")]
pub mod focus_steal;
#[cfg(target_os = "macos")]
pub mod history;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
mod permission_observation;
#[cfg(target_os = "macos")]
pub mod permissions;
#[cfg(target_os = "macos")]
pub mod pip;
#[cfg(target_os = "macos")]
pub mod recording_hooks;
#[cfg(target_os = "macos")]
pub mod session;
#[cfg(target_os = "macos")]
pub mod terminal;
#[cfg(target_os = "macos")]
pub mod tools;
#[cfg(target_os = "macos")]
#[path = "video_sckit_monterey.rs"]
pub mod video_sckit;
#[cfg(target_os = "macos")]
pub mod window_change_detector;
#[cfg(target_os = "macos")]
pub mod windows;

use cua_driver_core::tool::ToolRegistry;

pub fn register_tools() -> ToolRegistry {
    register_tools_with_compat(false)
}

pub fn register_tools_with_compat(compat: bool) -> ToolRegistry {
    #[cfg(target_os = "macos")]
    {
        let mut r = ToolRegistry::new();
        tools::register_all(&mut r, compat, false, false, None);
        r
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = compat;
        ToolRegistry::new()
    }
}

pub fn register_tools_with_cursor(
    cfg: cursor_overlay::CursorConfig,
    compat: bool,
    host_owns_permission_ux: bool,
    host_bundle_id: Option<String>,
) -> ToolRegistry {
    register_tools_with_cursor_and_provider(
        None,
        cfg,
        compat,
        host_owns_permission_ux,
        host_bundle_id,
    )
}

pub fn register_tools_with_cursor_and_provider(
    provider: Option<std::sync::Arc<dyn cua_driver_core::consent::ProtectedConsentProvider>>,
    cfg: cursor_overlay::CursorConfig,
    compat: bool,
    host_owns_permission_ux: bool,
    host_bundle_id: Option<String>,
) -> ToolRegistry {
    #[cfg(target_os = "macos")]
    {
        let cursor_overlay_available =
            cursor_overlay_facility_available(cfg.enabled, session::has_graphic_access());
        if cursor_overlay_available {
            cursor::overlay::init(cfg);
        }
        let mut r = ToolRegistry::new_with_protected_consent_provider(provider);
        tools::register_all(
            &mut r,
            compat,
            cursor_overlay_available,
            host_owns_permission_ux,
            host_bundle_id,
        );
        r
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cfg;
        let _ = compat;
        let _ = host_owns_permission_ux;
        let _ = host_bundle_id;
        let _ = provider;
        ToolRegistry::new()
    }
}

#[cfg(target_os = "macos")]
fn cursor_overlay_facility_available(enabled: bool, graphic_access: bool) -> bool {
    enabled && graphic_access
}

#[cfg(all(test, target_os = "macos"))]
mod cursor_overlay_host_tests {
    use super::cursor_overlay_facility_available;

    #[test]
    fn overlay_requires_both_host_enablement_and_graphic_session_access() {
        assert!(cursor_overlay_facility_available(true, true));
        assert!(!cursor_overlay_facility_available(false, true));
        assert!(!cursor_overlay_facility_available(true, false));
        assert!(!cursor_overlay_facility_available(false, false));
    }
}
