//! Stable C ABI below the public typed SDK.
//!
//! The exported functions in this module are deliberately smaller and more
//! conservative than UniFFI's generated ABI. They use versioned symbols,
//! opaque handles, caller-visible ownership, and status codes that a future
//! non-Rust native core can reproduce without implementing UniFFI internals.

use crate::runtime::{DriverRuntime, RuntimeCreateError, RuntimeOptions, RuntimeSession};
use crate::{DriverError, DriverMetadata};
use cua_driver_core::{
    authorization::{
        PermissionMode, DANGEROUS_BYPASS_ENV, DISABLE_UNRESTRICTED_ENV, PERMISSION_MODE_ENV,
    },
    session_authorization::{DelegatedSessionRequest, SessionModeCeiling},
    session_manifest::{
        load_manifest, CAPABILITY_MANIFEST_APPROVED_ENV, CAPABILITY_MANIFEST_FILE_ENV,
        SESSION_POLICY_APPROVED_ENV, SESSION_POLICY_FILE_ENV,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::ffi::c_void;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;
use tokio::sync::{oneshot, Notify};

pub const CUA_DRIVER_ABI_MAJOR: u16 = 1;
pub const CUA_DRIVER_ABI_MINOR: u16 = 1;
pub const CUA_DRIVER_ABI_PATCH: u16 = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Version of the implementation-neutral Cua Driver ABI.
pub struct CuaDriverAbiVersion {
    pub struct_size: u32,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub reserved: u16,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Stable status codes returned across the C ABI.
pub enum CuaDriverStatus {
    Ok = 0,
    InvalidArgument = 1,
    NullPointer = 2,
    RuntimeUnavailable = 3,
    Shutdown = 4,
    Cancelled = 5,
    Internal = 6,
    Panic = 7,
    RuntimeConflict = 8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Caller-owned byte buffer returned by the ABI.
/// Pass it to `cua_driver_buffer_free_v1`; freeing an empty buffer is harmless.
pub struct CuaDriverBuffer {
    pub data: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl CuaDriverBuffer {
    const fn empty() -> Self {
        Self {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }

    fn from_string(value: String) -> Self {
        if value.is_empty() {
            return Self::empty();
        }
        let mut bytes = value.into_bytes();
        let buffer = Self {
            data: bytes.as_mut_ptr(),
            len: bytes.len(),
            capacity: bytes.capacity(),
        };
        std::mem::forget(bytes);
        buffer
    }
}

/// Opaque driver runtime handle.
pub struct CuaDriverHandle {
    runtime: Arc<DriverRuntime>,
}

/// Opaque session handle whose actions are already bound to one immutable
/// authorization context.
pub struct CuaDriverSessionHandle {
    session: Arc<RuntimeSession>,
}

struct OperationState {
    cancelled: AtomicBool,
    changed: Notify,
}

impl OperationState {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.changed.notify_one();
    }

    async fn cancelled(&self) {
        loop {
            let changed = self.changed.notified();
            if self.cancelled.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    }
}

/// Opaque token for one asynchronous operation.
pub struct CuaDriverOperation {
    state: Arc<OperationState>,
}

/// Completion callback for asynchronous operations. It is called exactly once
/// unless the process terminates. Result and error buffers are caller-owned.
pub type CuaDriverCompletionV1 = extern "C" fn(
    context: *mut c_void,
    status: CuaDriverStatus,
    result: CuaDriverBuffer,
    error: CuaDriverBuffer,
);

#[derive(Debug)]
struct AbiFailure {
    status: CuaDriverStatus,
    message: String,
}

impl AbiFailure {
    fn new(status: CuaDriverStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AbiDriverOptions {
    claude_code_compatibility: bool,
    authorization: Option<AbiRuntimeAuthorizationOptions>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AbiRuntimeAuthorizationOptions {
    allowed_modes: Vec<PermissionMode>,
    compatibility_mode: PermissionMode,
    #[serde(default)]
    compatibility_capability_manifest_path: Option<String>,
    #[serde(default)]
    compatibility_bounded_manifest_path: Option<String>,
    unrestricted_acknowledged: bool,
    max_session_ttl_seconds: u64,
    max_idle_ttl_seconds: u64,
}

impl AbiRuntimeAuthorizationOptions {
    fn capability_manifest_path(&self) -> Result<Option<&str>, AbiFailure> {
        if self.compatibility_capability_manifest_path.is_some()
            && self.compatibility_bounded_manifest_path.is_some()
            && self.compatibility_capability_manifest_path
                != self.compatibility_bounded_manifest_path
        {
            return Err(AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                "compatibility capability manifest path conflicts with its deprecated bounded-manifest alias",
            ));
        }
        Ok(self
            .compatibility_capability_manifest_path
            .as_deref()
            .or(self.compatibility_bounded_manifest_path.as_deref()))
    }
}

fn validate_explicit_authorization_sources(
    authorization: &AbiRuntimeAuthorizationOptions,
) -> Result<(), AbiFailure> {
    cua_driver_core::policy::validate_configured_policy().map_err(|error| {
        AbiFailure::new(
            CuaDriverStatus::InvalidArgument,
            format!("configured policy is invalid: {error}"),
        )
    })?;
    if std::env::var_os(PERMISSION_MODE_ENV).is_some()
        || std::env::var_os(DANGEROUS_BYPASS_ENV).is_some()
    {
        let compatibility_mode =
            cua_driver_core::authorization::configured_permission_mode().map_err(|error| {
                AbiFailure::new(
                    CuaDriverStatus::InvalidArgument,
                    format!(
                        "explicit runtime authorization conflicts with invalid compatibility environment: {error}"
                    ),
                )
            })?;
        if compatibility_mode != authorization.compatibility_mode {
            return Err(AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                format!(
                    "explicit compatibility mode '{}' conflicts with trusted environment mode '{}'",
                    authorization.compatibility_mode.as_str(),
                    compatibility_mode.as_str(),
                ),
            ));
        }
    }

    if authorization
        .allowed_modes
        .contains(&PermissionMode::Unrestricted)
        && environment_flag(DISABLE_UNRESTRICTED_ENV)
    {
        return Err(AbiFailure::new(
            CuaDriverStatus::InvalidArgument,
            "explicit runtime ceiling conflicts with managed configuration disabling unrestricted mode",
        ));
    }

    let capability_environment_manifest = std::env::var_os(CAPABILITY_MANIFEST_FILE_ENV);
    let legacy_environment_manifest = std::env::var_os(SESSION_POLICY_FILE_ENV);
    if capability_environment_manifest.is_some()
        && legacy_environment_manifest.is_some()
        && capability_environment_manifest != legacy_environment_manifest
    {
        return Err(AbiFailure::new(
            CuaDriverStatus::InvalidArgument,
            "capability manifest environment path conflicts with its deprecated session-policy alias",
        ));
    }
    let environment_manifest = capability_environment_manifest
        .or(legacy_environment_manifest)
        .map(std::path::PathBuf::from);
    if let Some(environment_manifest) = environment_manifest {
        let Some(explicit_manifest) = authorization
            .capability_manifest_path()?
            .map(std::path::Path::new)
        else {
            return Err(AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                "explicit runtime authorization conflicts with a compatibility capability-manifest environment value",
            ));
        };
        let environment_manifest =
            std::fs::canonicalize(&environment_manifest).map_err(|error| {
                AbiFailure::new(
                    CuaDriverStatus::InvalidArgument,
                    format!(
                        "canonicalize compatibility capability manifest {}: {error}",
                        environment_manifest.display()
                    ),
                )
            })?;
        let explicit_manifest = std::fs::canonicalize(explicit_manifest).map_err(|error| {
            AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                format!(
                    "canonicalize explicit compatibility capability manifest {}: {error}",
                    explicit_manifest.display()
                ),
            )
        })?;
        if environment_manifest != explicit_manifest {
            return Err(AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                "explicit compatibility capability manifest conflicts with the trusted environment path",
            ));
        }
    } else if (environment_flag(CAPABILITY_MANIFEST_APPROVED_ENV)
        || environment_flag(SESSION_POLICY_APPROVED_ENV))
        && authorization.capability_manifest_path()?.is_none()
    {
        return Err(AbiFailure::new(
            CuaDriverStatus::InvalidArgument,
            "explicit runtime authorization conflicts with a compatibility capability-manifest approval",
        ));
    }
    Ok(())
}

fn runtime_options_from_abi(options: AbiDriverOptions) -> Result<RuntimeOptions, AbiFailure> {
    match options.authorization {
        Some(authorization) => {
            validate_explicit_authorization_sources(&authorization)?;
            let ceiling = SessionModeCeiling::for_trusted_sessions(
                authorization.allowed_modes.clone(),
                authorization.unrestricted_acknowledged,
                Duration::from_secs(authorization.max_session_ttl_seconds),
                Duration::from_secs(authorization.max_idle_ttl_seconds),
            )
            .map_err(|error| AbiFailure::new(CuaDriverStatus::InvalidArgument, error))?;
            let manifest = authorization
                .capability_manifest_path()?
                .map(std::path::Path::new)
                .map(load_manifest)
                .transpose()
                .map_err(|error| AbiFailure::new(CuaDriverStatus::InvalidArgument, error))?
                .map(Arc::new);
            Ok(RuntimeOptions::embedded_with_ceiling(
                options.claude_code_compatibility,
                ceiling,
                authorization.compatibility_mode,
                manifest,
            ))
        }
        None => Ok(RuntimeOptions::embedded(options.claude_code_compatibility)),
    }
}

fn environment_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AbiTrustedSessionOptions {
    public_session: String,
    mode: PermissionMode,
    ttl_seconds: u64,
    idle_ttl_seconds: u64,
    #[serde(default)]
    capability_manifest_path: Option<String>,
    #[serde(default)]
    bounded_manifest_path: Option<String>,
    #[serde(default)]
    transport_session: Option<String>,
}

impl AbiTrustedSessionOptions {
    fn capability_manifest_path(&self) -> Result<Option<&str>, AbiFailure> {
        if self.capability_manifest_path.is_some()
            && self.bounded_manifest_path.is_some()
            && self.capability_manifest_path != self.bounded_manifest_path
        {
            return Err(AbiFailure::new(
                CuaDriverStatus::InvalidArgument,
                "capability manifest path conflicts with its deprecated bounded-manifest alias",
            ));
        }
        Ok(self
            .capability_manifest_path
            .as_deref()
            .or(self.bounded_manifest_path.as_deref()))
    }
}

fn with_ffi_guard(
    out_error: *mut CuaDriverBuffer,
    operation: impl FnOnce() -> Result<(), AbiFailure>,
) -> CuaDriverStatus {
    unsafe {
        if !out_error.is_null() {
            *out_error = CuaDriverBuffer::empty();
        }
    }
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => CuaDriverStatus::Ok,
        Ok(Err(error)) => {
            unsafe { write_error(out_error, error.message) };
            error.status
        }
        Err(_) => {
            unsafe {
                write_error(
                    out_error,
                    "the native Cua Driver core panicked; no panic crossed the C ABI".into(),
                )
            };
            CuaDriverStatus::Panic
        }
    }
}

unsafe fn write_error(out_error: *mut CuaDriverBuffer, message: String) {
    if !out_error.is_null() {
        *out_error = CuaDriverBuffer::from_string(message);
    }
}

unsafe fn input_bytes<'a>(data: *const u8, len: usize) -> Result<&'a [u8], AbiFailure> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(AbiFailure::new(
            CuaDriverStatus::NullPointer,
            "non-empty input used a null data pointer",
        ));
    }
    Ok(std::slice::from_raw_parts(data, len))
}

unsafe fn required_handle<'a>(
    handle: *mut CuaDriverHandle,
) -> Result<&'a CuaDriverHandle, AbiFailure> {
    handle.as_ref().ok_or_else(|| {
        AbiFailure::new(
            CuaDriverStatus::NullPointer,
            "driver handle must not be null",
        )
    })
}

unsafe fn required_session_handle<'a>(
    handle: *mut CuaDriverSessionHandle,
) -> Result<&'a CuaDriverSessionHandle, AbiFailure> {
    handle.as_ref().ok_or_else(|| {
        AbiFailure::new(
            CuaDriverStatus::NullPointer,
            "session handle must not be null",
        )
    })
}

fn metadata_json() -> Result<String, AbiFailure> {
    serde_json::to_string(&DriverMetadata {
        driver_version: env!("CARGO_PKG_VERSION").into(),
        contract_version: cua_driver_contract::CONTRACT_VERSION.into(),
        tools_list_schema_version: cua_driver_contract::TOOLS_LIST_SCHEMA_VERSION.into(),
        capability_version: cua_driver_contract::CAPABILITY_VERSION.into(),
        mcp_protocol_version: cua_driver_contract::MCP_PROTOCOL_VERSION.into(),
        pid: std::process::id(),
        embedded: true,
        host_bundle_id: None,
    })
    .map_err(|error| AbiFailure::new(CuaDriverStatus::Internal, error.to_string()))
}

fn abi_executor() -> Result<&'static tokio::runtime::Runtime, AbiFailure> {
    static EXECUTOR: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    match EXECUTOR.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("cua-driver-abi")
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => Err(AbiFailure::new(
            CuaDriverStatus::RuntimeUnavailable,
            error.clone(),
        )),
    }
}

fn complete_immediately(
    callback: Option<CuaDriverCompletionV1>,
    context: *mut c_void,
    status: CuaDriverStatus,
    result: CuaDriverBuffer,
    error: CuaDriverBuffer,
) {
    if let Some(callback) = callback {
        callback(context, status, result, error);
    }
}

unsafe fn spawn_operation<F>(
    callback: Option<CuaDriverCompletionV1>,
    context: *mut c_void,
    out_operation: *mut *mut CuaDriverOperation,
    out_error: *mut CuaDriverBuffer,
    future: F,
) -> CuaDriverStatus
where
    F: Future<Output = Result<String, DriverError>> + Send + 'static,
{
    with_ffi_guard(out_error, || {
        if out_operation.is_null() {
            return Err(AbiFailure::new(
                CuaDriverStatus::NullPointer,
                "out_operation must not be null",
            ));
        }
        let executor = abi_executor()?;
        let state = Arc::new(OperationState::new());
        let operation = Box::new(CuaDriverOperation {
            state: state.clone(),
        });
        *out_operation = Box::into_raw(operation);
        let context_addr = context as usize;
        executor.spawn(async move {
            let context = context_addr as *mut c_void;
            let result = tokio::select! {
                _ = state.cancelled() => Err(DriverError::Cancelled),
                result = future => result,
            };
            match result {
                Ok(value) => complete_immediately(
                    callback,
                    context,
                    CuaDriverStatus::Ok,
                    CuaDriverBuffer::from_string(value),
                    CuaDriverBuffer::empty(),
                ),
                Err(DriverError::Cancelled) => complete_immediately(
                    callback,
                    context,
                    CuaDriverStatus::Cancelled,
                    CuaDriverBuffer::empty(),
                    CuaDriverBuffer::empty(),
                ),
                Err(error) => complete_immediately(
                    callback,
                    context,
                    CuaDriverStatus::Internal,
                    CuaDriverBuffer::empty(),
                    CuaDriverBuffer::from_string(error.to_string()),
                ),
            }
        });
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cua_driver_abi_version_v1() -> CuaDriverAbiVersion {
    CuaDriverAbiVersion {
        struct_size: std::mem::size_of::<CuaDriverAbiVersion>() as u32,
        major: CUA_DRIVER_ABI_MAJOR,
        minor: CUA_DRIVER_ABI_MINOR,
        patch: CUA_DRIVER_ABI_PATCH,
        reserved: 0,
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_buffer_free_v1(buffer: *mut CuaDriverBuffer) {
    if buffer.is_null() {
        return;
    }
    unsafe {
        let buffer = &mut *buffer;
        if !buffer.data.is_null() && buffer.capacity > 0 {
            let _ = Vec::from_raw_parts(buffer.data, buffer.len, buffer.capacity);
        }
        *buffer = CuaDriverBuffer::empty();
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_metadata_v1(
    out_metadata: *mut CuaDriverBuffer,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    with_ffi_guard(out_error, || {
        if out_metadata.is_null() {
            return Err(AbiFailure::new(
                CuaDriverStatus::NullPointer,
                "out_metadata must not be null",
            ));
        }
        unsafe { *out_metadata = CuaDriverBuffer::from_string(metadata_json()?) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cua_driver_create_v1(
    options_json: *const u8,
    options_len: usize,
    out_handle: *mut *mut CuaDriverHandle,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    with_ffi_guard(out_error, || {
        if out_handle.is_null() {
            return Err(AbiFailure::new(
                CuaDriverStatus::NullPointer,
                "out_handle must not be null",
            ));
        }
        let bytes = unsafe { input_bytes(options_json, options_len)? };
        let options: AbiDriverOptions = if bytes.is_empty() {
            AbiDriverOptions::default()
        } else {
            serde_json::from_slice(bytes)
                .map_err(|error| AbiFailure::new(CuaDriverStatus::InvalidArgument, error.to_string()))?
        };
        let runtime = DriverRuntime::new(runtime_options_from_abi(options)?)
            .map_err(|error| match error {
                RuntimeCreateError::AlreadyActive => AbiFailure::new(
                    CuaDriverStatus::RuntimeConflict,
                    "another embedded Cua Driver runtime is already active",
                ),
                other => AbiFailure::new(CuaDriverStatus::RuntimeUnavailable, other.to_string()),
            })?;
        let handle = Box::new(CuaDriverHandle {
            runtime: Arc::new(runtime),
        });
        unsafe { *out_handle = Box::into_raw(handle) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cua_driver_destroy_v1(handle: *mut *mut CuaDriverHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        if !(*handle).is_null() {
            drop(Box::from_raw(*handle));
            *handle = ptr::null_mut();
        }
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_invoke_v1(
    handle: *mut CuaDriverHandle,
    name: *const u8,
    name_len: usize,
    arguments_json: *const u8,
    arguments_len: usize,
    callback: Option<CuaDriverCompletionV1>,
    context: *mut c_void,
    out_operation: *mut *mut CuaDriverOperation,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    unsafe {
        let handle = match required_handle(handle) {
            Ok(handle) => handle,
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let name = match input_bytes(name, name_len)
            .and_then(|bytes| std::str::from_utf8(bytes).map_err(|e| AbiFailure::new(CuaDriverStatus::InvalidArgument, e.to_string())))
        {
            Ok(name) => name.to_owned(),
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let arguments = match input_bytes(arguments_json, arguments_len).and_then(|bytes| {
            if bytes.is_empty() {
                Ok(Value::Object(Default::default()))
            } else {
                serde_json::from_slice(bytes)
                    .map_err(|e| AbiFailure::new(CuaDriverStatus::InvalidArgument, e.to_string()))
            }
        }) {
            Ok(arguments) => arguments,
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let runtime = handle.runtime.clone();
        spawn_operation(callback, context, out_operation, out_error, async move {
            runtime.invoke(&name, arguments).await
        })
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_session_create_v1(
    handle: *mut CuaDriverHandle,
    options_json: *const u8,
    options_len: usize,
    out_session: *mut *mut CuaDriverSessionHandle,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    with_ffi_guard(out_error, || {
        if out_session.is_null() {
            return Err(AbiFailure::new(
                CuaDriverStatus::NullPointer,
                "out_session must not be null",
            ));
        }
        let handle = unsafe { required_handle(handle)? };
        let bytes = unsafe { input_bytes(options_json, options_len)? };
        let options: AbiTrustedSessionOptions = serde_json::from_slice(bytes)
            .map_err(|error| AbiFailure::new(CuaDriverStatus::InvalidArgument, error.to_string()))?;
        let request = DelegatedSessionRequest {
            public_session: options.public_session,
            transport_session: options.transport_session,
            mode: options.mode,
            ttl_seconds: options.ttl_seconds,
            idle_ttl_seconds: options.idle_ttl_seconds,
            capability_manifest_path: options.capability_manifest_path,
            bounded_manifest_path: options.bounded_manifest_path,
        };
        let session = handle
            .runtime
            .create_session(request)
            .map_err(|error| AbiFailure::new(CuaDriverStatus::InvalidArgument, error.to_string()))?;
        unsafe {
            *out_session = Box::into_raw(Box::new(CuaDriverSessionHandle {
                session: Arc::new(session),
            }));
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cua_driver_session_destroy_v1(handle: *mut *mut CuaDriverSessionHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        if !(*handle).is_null() {
            drop(Box::from_raw(*handle));
            *handle = ptr::null_mut();
        }
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_session_invoke_v1(
    handle: *mut CuaDriverSessionHandle,
    name: *const u8,
    name_len: usize,
    arguments_json: *const u8,
    arguments_len: usize,
    callback: Option<CuaDriverCompletionV1>,
    context: *mut c_void,
    out_operation: *mut *mut CuaDriverOperation,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    unsafe {
        let session = match required_session_handle(handle) {
            Ok(session) => session,
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let name = match input_bytes(name, name_len)
            .and_then(|bytes| std::str::from_utf8(bytes).map_err(|e| AbiFailure::new(CuaDriverStatus::InvalidArgument, e.to_string())))
        {
            Ok(name) => name.to_owned(),
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let arguments = match input_bytes(arguments_json, arguments_len).and_then(|bytes| {
            if bytes.is_empty() {
                Ok(Value::Object(Default::default()))
            } else {
                serde_json::from_slice(bytes)
                    .map_err(|e| AbiFailure::new(CuaDriverStatus::InvalidArgument, e.to_string()))
            }
        }) {
            Ok(arguments) => arguments,
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let session = session.session.clone();
        spawn_operation(callback, context, out_operation, out_error, async move {
            session.invoke(&name, arguments).await
        })
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_shutdown_v1(
    handle: *mut CuaDriverHandle,
    callback: Option<CuaDriverCompletionV1>,
    context: *mut c_void,
    out_operation: *mut *mut CuaDriverOperation,
    out_error: *mut CuaDriverBuffer,
) -> CuaDriverStatus {
    unsafe {
        let handle = match required_handle(handle) {
            Ok(handle) => handle,
            Err(error) => {
                write_error(out_error, error.message);
                return error.status;
            }
        };
        let runtime = handle.runtime.clone();
        spawn_operation(callback, context, out_operation, out_error, async move {
            runtime.shutdown().await;
            Ok("{}".to_owned())
        })
    }
}

#[no_mangle]
pub extern "C" fn cua_driver_operation_cancel_v1(operation: *mut CuaDriverOperation) {
    if operation.is_null() {
        return;
    }
    unsafe { (*operation).state.cancel() };
}

#[no_mangle]
pub extern "C" fn cua_driver_operation_release_v1(operation: *mut *mut CuaDriverOperation) {
    if operation.is_null() {
        return;
    }
    unsafe {
        if !(*operation).is_null() {
            drop(Box::from_raw(*operation));
            *operation = ptr::null_mut();
        }
    }
}

pub mod client {
    use super::*;

    pub struct Handle {
        raw: *mut CuaDriverHandle,
    }

    pub struct Session {
        raw: *mut CuaDriverSessionHandle,
    }

    pub struct Operation {
        raw: *mut CuaDriverOperation,
    }

    mod ffi {
        use super::*;

        extern "C" {
            #[link_name = "cua_driver_abi_version_v1"]
            pub(super) fn abi_version() -> CuaDriverAbiVersion;
            #[link_name = "cua_driver_buffer_free_v1"]
            pub(super) fn buffer_free(buffer: *mut CuaDriverBuffer);
            #[link_name = "cua_driver_metadata_v1"]
            pub(super) fn metadata(
                out_metadata: *mut CuaDriverBuffer,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[link_name = "cua_driver_create_v1"]
            pub(super) fn create(
                options_json: *const u8,
                options_len: usize,
                out_handle: *mut *mut Handle,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[link_name = "cua_driver_destroy_v1"]
            pub(super) fn destroy(handle: *mut *mut Handle);
            #[link_name = "cua_driver_invoke_v1"]
            pub(super) fn invoke(
                handle: *mut Handle,
                name: *const u8,
                name_len: usize,
                arguments_json: *const u8,
                arguments_len: usize,
                callback: Option<CuaDriverCompletionV1>,
                context: *mut c_void,
                out_operation: *mut *mut Operation,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[link_name = "cua_driver_session_create_v1"]
            pub(super) fn session_create(
                handle: *mut Handle,
                options_json: *const u8,
                options_len: usize,
                out_session: *mut *mut Session,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[link_name = "cua_driver_session_destroy_v1"]
            pub(super) fn session_destroy(handle: *mut *mut Session);
            #[link_name = "cua_driver_session_invoke_v1"]
            pub(super) fn session_invoke(
                handle: *mut Session,
                name: *const u8,
                name_len: usize,
                arguments_json: *const u8,
                arguments_len: usize,
                callback: Option<CuaDriverCompletionV1>,
                context: *mut c_void,
                out_operation: *mut *mut Operation,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[allow(dead_code)]
            #[link_name = "cua_driver_shutdown_v1"]
            pub(super) fn shutdown(
                handle: *mut Handle,
                callback: Option<CuaDriverCompletionV1>,
                context: *mut c_void,
                out_operation: *mut *mut Operation,
                out_error: *mut CuaDriverBuffer,
            ) -> CuaDriverStatus;
            #[link_name = "cua_driver_operation_cancel_v1"]
            pub(super) fn operation_cancel(operation: *mut Operation);
            #[link_name = "cua_driver_operation_release_v1"]
            pub(super) fn operation_release(operation: *mut *mut Operation);
        }
    }
}

unsafe fn copy_and_free_buffer(buffer: &mut CuaDriverBuffer) -> String {
    let value = if buffer.data.is_null() || buffer.len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(std::slice::from_raw_parts(buffer.data, buffer.len)).into_owned()
    };
    ffi::buffer_free(buffer);
    value
}
