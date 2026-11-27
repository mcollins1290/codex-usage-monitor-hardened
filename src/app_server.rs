//! Restricted Codex app-server client.
//!
//! This module intentionally exposes one operation only: a bounded, read-only
//! `account/rateLimits/read` request against an explicitly approved executable.
//! It does not read credentials, open sockets, invoke a shell, or provide a
//! generic JSON-RPC escape hatch.

use crate::models::{UsageData, UsageSection};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_JSON_LINE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_OUTPUT_BYTES: usize = 1024 * 1024;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(7);
const CODEX_VERSION: &str = "0.172.0";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApprovedExecutable {
    pub path: PathBuf,
    pub sha256: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppServerError {
    StartupNotVerified,
    InvalidIdentity,
    ExecutableMissing,
    HashMismatch,
    LaunchFailed,
    Timeout,
    OutputTooLarge,
    InvalidMessage,
    UnexpectedMessage,
    ProtocolError,
    InvalidQuota,
}

#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
struct IoCounters {
    values: [u64; 6],
}

#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic: JobObjectBasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x0000_0008;

unsafe extern "system" {
    fn CreateJobObjectW(
        attributes: *mut std::ffi::c_void,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn SetInformationJobObject(
        job: *mut std::ffi::c_void,
        class: u32,
        info: *mut std::ffi::c_void,
        length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: *mut std::ffi::c_void, process: *mut std::ffi::c_void) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

struct JobGuard(*mut std::ffi::c_void);
impl JobGuard {
    fn attach(child: &std::process::Child) -> Result<Self, AppServerError> {
        let job = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if job.is_null() {
            return Err(AppServerError::LaunchFailed);
        }
        let mut limits = JobObjectExtendedLimitInformation {
            basic: JobObjectBasicLimitInformation {
                per_process_user_time_limit: 0,
                per_job_user_time_limit: 0,
                limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
                minimum_working_set_size: 0,
                maximum_working_set_size: 0,
                active_process_limit: 1,
                affinity: 0,
                priority_class: 0,
                scheduling_class: 0,
            },
            io: IoCounters { values: [0; 6] },
            process_memory_limit: 0,
            job_memory_limit: 0,
            peak_process_memory_used: 0,
            peak_job_memory_used: 0,
        };
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&mut limits as *mut JobObjectExtendedLimitInformation).cast::<std::ffi::c_void>(),
                std::mem::size_of_val(&limits) as u32,
            )
        } != 0;
        let assigned = configured
            && unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as *mut _) } != 0;
        if !assigned {
            unsafe {
                CloseHandle(job);
            }
            return Err(AppServerError::LaunchFailed);
        }
        Ok(Self(job))
    }
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Deserialize)]
struct RateLimitsResult {
    #[serde(rename = "rateLimitsByLimitId")]
    by_limit_id: Option<std::collections::HashMap<String, RateLimitSnapshot>>,
    #[serde(rename = "rateLimits")]
    rate_limits: Option<RateLimitSnapshot>,
}

#[derive(Debug, Deserialize, Clone)]
struct RateLimitSnapshot {
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize, Clone)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: i32,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<u64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

pub fn expected_version() -> &'static str {
    CODEX_VERSION
}

pub fn sha256_file(path: &Path) -> Result<String, AppServerError> {
    let mut file = File::open(path).map_err(|_| AppServerError::ExecutableMissing)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|_| AppServerError::LaunchFailed)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

pub fn validate_identity(identity: &ApprovedExecutable) -> Result<PathBuf, AppServerError> {
    if !identity.path.is_absolute()
        || identity.sha256.len() != 64
        || identity.version != CODEX_VERSION
    {
        return Err(AppServerError::InvalidIdentity);
    }
    if !identity.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppServerError::InvalidIdentity);
    }
    let canonical =
        std::fs::canonicalize(&identity.path).map_err(|_| AppServerError::ExecutableMissing)?;
    // Approval must persist the canonical path, not a mutable alias.
    if canonical != identity.path || !canonical.is_file() {
        return Err(AppServerError::InvalidIdentity);
    }
    let actual = sha256_file(&canonical)?;
    if !actual.eq_ignore_ascii_case(&identity.sha256) {
        return Err(AppServerError::HashMismatch);
    }
    Ok(canonical)
}

fn require_startup_acceptance() -> Result<(), AppServerError> {
    Err(AppServerError::StartupNotVerified)
}

/// Retrieve quota data using only the fixed app-server request sequence.
pub fn read_rate_limits(identity: &ApprovedExecutable) -> Result<UsageData, AppServerError> {
    // Deliberately not configurable: no authenticated invocation has been accepted.
    require_startup_acceptance()?;
    let executable = validate_identity(identity)?;
    let workdir = dirs::config_dir()
        .ok_or(AppServerError::LaunchFailed)?
        .join("CodexUsageHardened")
        .join("app-server-work");
    std::fs::create_dir_all(&workdir).map_err(|_| AppServerError::LaunchFailed)?;
    let mut command = Command::new(&executable);
    command
        .args(["app-server", "--listen", "stdio://"])
        .creation_flags(CREATE_NO_WINDOW)
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for name in [
        "CODEX_HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "SystemRoot",
        "WINDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }

    // The launch freeze stays in place until creation-time job assignment and
    // the authenticated environment policy have been verified.
    let mut guard = ChildGuard {
        child: command.spawn().map_err(|_| AppServerError::LaunchFailed)?,
        job: None,
    };
    guard.job = Some(JobGuard::attach(&guard.child)?);
    let mut stdin = guard.child.stdin.take().ok_or(AppServerError::LaunchFailed)?;
    let stdout = guard.child.stdout.take().ok_or(AppServerError::LaunchFailed)?;
    let stderr = guard.child.stderr.take().ok_or(AppServerError::LaunchFailed)?;

    let (tx, rx) = sync_channel(16);
    let output_limit = Arc::new(AtomicBool::new(false));
    let stdout_limit = Arc::clone(&output_limit);
    let stdout_thread = std::thread::spawn(move || read_stdout(stdout, tx, stdout_limit));
    let stderr_limit = Arc::clone(&output_limit);
    let stderr_thread = std::thread::spawn(move || drain_stderr(stderr, stderr_limit));

    let result = exchange(&mut stdin, |id, deadline| {
        receive_response(&rx, &output_limit, id, deadline)
    });
    drop(stdin);
    // Drop the receiver before cleanup: even an output flood cannot leave a
    // producer blocked on a full queue while the controller waits for it.
    drop(rx);
    let shutdown = guard.shutdown(result.is_ok());
    let readers = finish_readers([stdout_thread, stderr_thread]);
    shutdown?;
    readers?;
    if output_limit.load(Ordering::Acquire) {
        return Err(AppServerError::OutputTooLarge);
    }
    quota_data(result?)
}

struct ChildGuard {
    child: std::process::Child,
    job: Option<JobGuard>,
}

impl ChildGuard {
    fn shutdown(&mut self, graceful: bool) -> Result<(), AppServerError> {
        if graceful && self.wait_until(Instant::now() + SHUTDOWN_TIMEOUT)? {
            self.job.take();
            return Ok(());
        }
        // Closing the job kills members before waiting for pipes to close.
        self.job.take();
        if self.child.try_wait().map_err(|_| AppServerError::LaunchFailed)?.is_none() {
            let _ = self.child.kill();
        }
        if self.wait_until(Instant::now() + SHUTDOWN_TIMEOUT)? {
            Ok(())
        } else {
            Err(AppServerError::Timeout)
        }
    }

    fn wait_until(&mut self, deadline: Instant) -> Result<bool, AppServerError> {
        loop {
            if self.child.try_wait().map_err(|_| AppServerError::LaunchFailed)?.is_some() {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.shutdown(false);
    }
}

fn finish_readers(readers: [std::thread::JoinHandle<()>; 2]) -> Result<(), AppServerError> {
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while !readers.iter().all(|reader| reader.is_finished()) {
        if Instant::now() >= deadline {
            // A descendant outside the job could retain a pipe. Do not hang
            // the monitor waiting for it; live operation remains blocked.
            return Err(AppServerError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for reader in readers {
        reader.join().map_err(|_| AppServerError::LaunchFailed)?;
    }
    Ok(())
}

fn send_message(writer: &mut impl Write, message: serde_json::Value) -> Result<(), AppServerError> {
    let line = serde_json::to_vec(&message).map_err(|_| AppServerError::ProtocolError)?;
    writer
        .write_all(&line)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|_| AppServerError::LaunchFailed)
}

// This is the only production protocol sender. Tests exercise this exact path.
fn exchange(
    writer: &mut impl Write,
    mut receive: impl FnMut(u64, Instant) -> Result<serde_json::Value, AppServerError>,
) -> Result<RateLimitsResult, AppServerError> {
    send_message(writer, serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"clientInfo": {"name": "codex-usage-monitor-hardened",
        "version": env!("CARGO_PKG_VERSION")}, "capabilities": {}}
    }))?;
    let initialized = receive(1, Instant::now() + STARTUP_TIMEOUT)?;
    if !initialized.is_object() {
        return Err(AppServerError::ProtocolError);
    }
    send_message(writer, serde_json::json!({
        "jsonrpc": "2.0", "method": "initialized", "params": {}
    }))?;
    send_message(writer, serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": {}
    }))?;
    serde_json::from_value(receive(2, Instant::now() + REQUEST_TIMEOUT)?)
        .map_err(|_| AppServerError::InvalidMessage)
}

fn receive_response(
    rx: &Receiver<Result<Vec<u8>, AppServerError>>,
    limit: &AtomicBool,
    id: u64,
    deadline: Instant,
) -> Result<serde_json::Value, AppServerError> {
    loop {
        if limit.load(Ordering::Acquire) {
            return Err(AppServerError::OutputTooLarge);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppServerError::Timeout);
        }
        let line = rx.recv_timeout(remaining).map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => AppServerError::Timeout,
            std::sync::mpsc::RecvTimeoutError::Disconnected => AppServerError::LaunchFailed,
        })??;
        let envelope: serde_json::Value =
            serde_json::from_slice(&line).map_err(|_| AppServerError::InvalidMessage)?;
        let object = envelope.as_object().ok_or(AppServerError::InvalidMessage)?;
        if let Some(version) = object.get("jsonrpc") {
            if version.as_str() != Some("2.0") {
                return Err(AppServerError::ProtocolError);
            }
        }
        match object.get("id") {
            None if object.get("method").and_then(|v| v.as_str()).is_some()
                && !object.contains_key("result") && !object.contains_key("error") => continue,
            Some(value) if value.as_u64() == Some(id) && !object.contains_key("method") => {
                if object.contains_key("error") {
                    return Err(AppServerError::ProtocolError);
                }
                return object.get("result").cloned().ok_or(AppServerError::ProtocolError);
            }
            _ => return Err(AppServerError::UnexpectedMessage),
        }
    }
}

// Enforce the byte limit before appending from the buffered reader. No
// unbounded read_line allocation is possible, even without a newline.
fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, AppServerError> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|_| AppServerError::LaunchFailed)?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(AppServerError::InvalidMessage)
            };
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if count > MAX_JSON_LINE_BYTES.saturating_sub(frame.len()) {
            return Err(AppServerError::OutputTooLarge);
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

fn read_stdout(
    stdout: impl Read,
    sender: SyncSender<Result<Vec<u8>, AppServerError>>,
    limit: Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(stdout);
    let mut total = 0usize;
    loop {
        match read_frame(&mut reader) {
            Ok(Some(frame)) => {
                total = total.saturating_add(frame.len());
                if total > MAX_TOTAL_OUTPUT_BYTES {
                    limit.store(true, Ordering::Release);
                    break;
                }
                match sender.try_send(Ok(frame)) {
                    Ok(()) => {}
                    Err(std::sync::mpsc::TrySendError::Full(_)) => {
                        limit.store(true, Ordering::Release);
                        break;
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
            Ok(None) => break,
            Err(error) => {
                if error == AppServerError::OutputTooLarge {
                    limit.store(true, Ordering::Release);
                }
                let _ = sender.try_send(Err(error));
                break;
            }
        }
    }
}

fn drain_stderr(mut stderr: impl Read, limit: Arc<AtomicBool>) {
    let mut total = 0usize;
    let mut buffer = [0u8; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                total = total.saturating_add(count);
                if total > MAX_TOTAL_OUTPUT_BYTES {
                    limit.store(true, Ordering::Release);
                    break;
                }
            }
        }
    }
}

fn quota_data(result: RateLimitsResult) -> Result<UsageData, AppServerError> {
    let snapshot = result
        .by_limit_id
        .as_ref()
        .and_then(|values| values.get("codex"))
        .or(result.rate_limits.as_ref())
        .ok_or(AppServerError::InvalidQuota)?;
    Ok(UsageData {
        session: window(snapshot.primary.as_ref(), true)?,
        weekly: window(snapshot.secondary.as_ref(), false)?,
        ..UsageData::default()
    })
}

fn window(window: Option<&RateLimitWindow>, session: bool) -> Result<UsageSection, AppServerError> {
    let Some(window) = window else {
        return Ok(UsageSection::default());
    };
    let duration = window
        .window_duration_mins
        .ok_or(AppServerError::InvalidQuota)?;
    let expected = if session { 300 } else { 10_080 };
    if duration != expected || !(0..=100).contains(&window.used_percent) {
        return Err(AppServerError::InvalidQuota);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default();
    let reset = window
        .resets_at
        .filter(|value| *value > now)
        .map(|value| UNIX_EPOCH + Duration::from_secs(value as u64));
    Ok(UsageSection::reported(window.used_percent as f64, reset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unverified_startup_cannot_launch_even_with_configured_identity() {
        let identity = ApprovedExecutable {
            path: PathBuf::from("not-an-executable"),
            sha256: "0".repeat(64),
            version: CODEX_VERSION.to_string(),
        };
        assert!(matches!(
            read_rate_limits(&identity),
            Err(AppServerError::StartupNotVerified)
        ));
    }

    #[test]
    fn protocol_sender_waits_for_initialization_and_emits_only_fixed_methods() {
        use std::cell::RefCell;
        struct RecordingWriter<'a>(&'a RefCell<Vec<u8>>);
        impl Write for RecordingWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let recorded = RefCell::new(Vec::new());
        let result = exchange(&mut RecordingWriter(&recorded), |id, _| {
            let bytes = recorded.borrow();
            let messages: Vec<serde_json::Value> = bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect();
            if id == 1 {
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0]["method"], "initialize");
                Ok(serde_json::json!({}))
            } else {
                assert_eq!(id, 2);
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[1]["method"], "initialized");
                assert_eq!(messages[2]["method"], "account/rateLimits/read");
                Ok(serde_json::json!({"rateLimits": {"primary": null, "secondary": null}}))
            }
        });
        assert!(result.is_ok());
    }

    #[test]
    fn failed_initialization_does_not_send_quota_request() {
        let mut writer = Vec::new();
        let result = exchange(&mut writer, |id, _| {
            assert_eq!(id, 1);
            Err(AppServerError::ProtocolError)
        });
        assert!(matches!(result, Err(AppServerError::ProtocolError)));
        assert_eq!(writer.iter().filter(|byte| **byte == b'\n').count(), 1);
    }

    #[test]
    fn frame_limit_is_enforced_without_newline() {
        let mut reader = std::io::Cursor::new(vec![b'x'; MAX_JSON_LINE_BYTES + 1]);
        assert_eq!(read_frame(&mut reader), Err(AppServerError::OutputTooLarge));
        let mut reader = std::io::Cursor::new(b"{}\n");
        assert_eq!(read_frame(&mut reader), Ok(Some(b"{}\n".to_vec())));
        assert_eq!(read_frame(&mut reader), Ok(None));
    }

    #[test]
    fn unexpected_response_id_is_rejected() {
        let (tx, rx) = sync_channel(1);
        tx.send(Ok(br#"{"id":2,"result":{}}"#.to_vec())).unwrap();
        assert_eq!(
            receive_response(&rx, &AtomicBool::new(false), 1, Instant::now() + STARTUP_TIMEOUT),
            Err(AppServerError::UnexpectedMessage)
        );
    }

    #[test]
    fn codex_bucket_is_selected_over_unrelated_buckets() {
        let result: RateLimitsResult = serde_json::from_value(serde_json::json!({
            "rateLimits": {"primary": null, "secondary": null},
            "rateLimitsByLimitId": {
                "other": {"primary": {"usedPercent": 99, "windowDurationMins": 300, "resetsAt": 2000000000}, "secondary": null},
                "codex": {"primary": {"usedPercent": 10, "windowDurationMins": 300, "resetsAt": 2000000000}, "secondary": {"usedPercent": 20, "windowDurationMins": 10080, "resetsAt": 2000000000}}
            }
        })).unwrap();
        let data = quota_data(result).unwrap();
        assert_eq!(data.session.percentage, 10.0);
        assert_eq!(data.weekly.percentage, 20.0);
    }

    #[test]
    fn identity_accepts_matching_absolute_file_and_rejects_changed_hash() {
        let path = std::env::temp_dir().join(format!("codex-identity-{}", std::process::id()));
        std::fs::write(&path, b"synthetic executable fixture").unwrap();
        let hash = sha256_file(&path).unwrap();
        let identity = ApprovedExecutable {
            path: path.canonicalize().unwrap(),
            sha256: hash,
            version: CODEX_VERSION.to_string(),
        };
        assert_eq!(
            validate_identity(&identity).unwrap(),
            path.canonicalize().unwrap()
        );

        let mut changed = identity.clone();
        let replacement = if changed.sha256.starts_with('0') { "1" } else { "0" };
        changed.sha256.replace_range(..1, replacement);
        assert_eq!(
            validate_identity(&changed),
            Err(AppServerError::HashMismatch)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn identity_rejects_relative_paths_and_unapproved_versions() {
        let relative = ApprovedExecutable {
            path: PathBuf::from("codex.exe"),
            sha256: "0".repeat(64),
            version: CODEX_VERSION.to_string(),
        };
        assert_eq!(
            validate_identity(&relative),
            Err(AppServerError::InvalidIdentity)
        );

        let wrong_version = ApprovedExecutable {
            path: PathBuf::from(r"C:\codex.exe"),
            sha256: "0".repeat(64),
            version: "0.167.0".to_string(),
        };
        assert_eq!(
            validate_identity(&wrong_version),
            Err(AppServerError::InvalidIdentity)
        );
    }

    #[test]
    fn missing_windows_are_unavailable_and_wrong_duration_is_rejected() {
        let missing: RateLimitsResult = serde_json::from_value(serde_json::json!({
            "rateLimits": {"primary": null, "secondary": null}
        }))
        .unwrap();
        let data = quota_data(missing).unwrap();
        assert!(!data.session.has_reading());
        assert!(!data.weekly.has_reading());

        let wrong: RateLimitsResult = serde_json::from_value(serde_json::json!({
            "rateLimits": {"primary": {"usedPercent": 10, "windowDurationMins": 60, "resetsAt": 2000000000}, "secondary": null}
        })).unwrap();
        assert!(matches!(
            quota_data(wrong),
            Err(AppServerError::InvalidQuota)
        ));
    }
}
