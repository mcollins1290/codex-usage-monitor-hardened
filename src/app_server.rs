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
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_JSON_LINE_BYTES: usize = 256 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(7);
const CODEX_VERSION: &str = "0.166.0";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApprovedExecutable {
    pub path: PathBuf,
    pub sha256: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppServerError {
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
                limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                minimum_working_set_size: 0,
                maximum_working_set_size: 0,
                active_process_limit: 0,
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
struct RpcEnvelope {
    id: Option<serde_json::Value>,
    result: Option<RateLimitsResult>,
    error: Option<serde_json::Value>,
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
    let canonical =
        std::fs::canonicalize(&identity.path).map_err(|_| AppServerError::ExecutableMissing)?;
    if canonical
        != identity
            .path
            .canonicalize()
            .map_err(|_| AppServerError::ExecutableMissing)?
    {
        return Err(AppServerError::InvalidIdentity);
    }
    let actual = sha256_file(&canonical)?;
    if !actual.eq_ignore_ascii_case(&identity.sha256) {
        return Err(AppServerError::HashMismatch);
    }
    Ok(canonical)
}

/// Retrieve quota data using only the fixed app-server request sequence.
pub fn read_rate_limits(identity: &ApprovedExecutable) -> Result<UsageData, AppServerError> {
    let executable = validate_identity(identity)?;
    let mut command = Command::new(&executable);
    command
        .arg("app-server")
        .arg("--stdio")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("CODEX_THREAD_ID")
        .env_clear();
    for name in [
        "CODEX_HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }

    let mut child = command.spawn().map_err(|_| AppServerError::LaunchFailed)?;
    let _job = JobGuard::attach(&child)?;
    let mut stdin = child.stdin.take().ok_or(AppServerError::LaunchFailed)?;
    let stdout = child.stdout.take().ok_or(AppServerError::LaunchFailed)?;
    let stderr = child.stderr.take().ok_or(AppServerError::LaunchFailed)?;

    // Drain stderr concurrently so a verbose client cannot block the protocol
    // reader. Its contents are deliberately discarded.
    let stderr_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let mut total = 0usize;
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            total = total.saturating_add(line.len());
            if total > MAX_JSON_LINE_BYTES {
                break;
            }
            line.clear();
        }
    });

    let messages = [
        serde_json::json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "codex-usage-monitor-hardened", "version": env!("CARGO_PKG_VERSION")}}}),
        serde_json::json!({"method": "initialized", "params": {}}),
        serde_json::json!({"id": 2, "method": "account/rateLimits/read", "params": {}}),
    ];
    for message in messages {
        let line = serde_json::to_vec(&message).map_err(|_| AppServerError::ProtocolError)?;
        stdin
            .write_all(&line)
            .map_err(|_| AppServerError::LaunchFailed)?;
        stdin
            .write_all(b"\n")
            .map_err(|_| AppServerError::LaunchFailed)?;
        stdin.flush().map_err(|_| AppServerError::LaunchFailed)?;
    }
    drop(stdin);

    let (line_tx, line_rx) = sync_channel(16);
    let stdout_thread = std::thread::spawn(move || read_stdout(stdout, line_tx));
    let deadline = std::time::Instant::now() + STARTUP_TIMEOUT + REQUEST_TIMEOUT;
    let mut result = None;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let line = match line_rx.recv_timeout(remaining) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                terminate(&mut child);
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(error);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let envelope: RpcEnvelope =
            serde_json::from_str(&line).map_err(|_| AppServerError::InvalidMessage)?;
        if envelope.id == Some(serde_json::json!(2)) {
            if envelope.error.is_some() {
                terminate(&mut child);
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(AppServerError::ProtocolError);
            }
            result = envelope.result;
            break;
        }
    }
    terminate(&mut child);
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    let result = result.ok_or(AppServerError::Timeout)?;
    quota_data(result)
}

fn read_stdout(
    stdout: impl std::io::Read,
    sender: std::sync::mpsc::SyncSender<Result<String, AppServerError>>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(size) if size > MAX_JSON_LINE_BYTES => {
                let _ = sender.send(Err(AppServerError::OutputTooLarge));
                break;
            }
            Ok(_) => {
                if sender.send(Ok(line)).is_err() {
                    break;
                }
            }
            Err(_) => {
                let _ = sender.send(Err(AppServerError::LaunchFailed));
                break;
            }
        }
    }
}

fn terminate(child: &mut std::process::Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
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
    fn expected_protocol_is_fixed_in_source() {
        let messages = ["initialize", "initialized", "account/rateLimits/read"];
        assert_eq!(messages.len(), 3);
        assert_eq!(expected_version(), "0.166.0");
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
            path: path.clone(),
            sha256: hash,
            version: CODEX_VERSION.to_string(),
        };
        assert_eq!(
            validate_identity(&identity).unwrap(),
            path.canonicalize().unwrap()
        );

        let mut changed = identity.clone();
        changed.sha256.replace_range(..1, "0");
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
