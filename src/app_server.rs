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
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_JSON_LINE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_OUTPUT_BYTES: usize = 1024 * 1024;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(7);
const COMPATIBILITY_SCHEMA: u32 = 2;
const INVOCATION_POLICY: &str = "account-only-stdio-v1";
#[cfg(test)]
const CODEX_VERSION: &str = "0.172.0";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedExecutable {
    pub path: PathBuf,
    pub sha256: String,
    pub version: String,
    #[serde(default)]
    pub compatibility: Option<CompatibilityRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityRecord {
    pub schema: u32,
    pub invocation_policy: String,
    pub release_tag: String,
    pub release_commit: String,
    pub observed_version: String,
    pub observed_sha256: String,
    #[serde(default)]
    pub observed_path: PathBuf,
    pub synthetic_passed: bool,
    pub authenticated_passed: bool,
    pub explicitly_approved: bool,
}

// Evidence is bound to the actual configured identity. This is a local
// administrative record, not a signature, sandbox or security trust anchor.
fn check_compatibility(identity: &ApprovedExecutable) -> Result<(), AppServerError> {
    let record = identity
        .compatibility
        .as_ref()
        .ok_or(AppServerError::CompatibilityRequired)?;
    if record.schema != COMPATIBILITY_SCHEMA
        || record.invocation_policy != INVOCATION_POLICY
        || record.release_tag != format!("rust-v{}", identity.version)
        || record.release_commit.len() != 40
        || !record
            .release_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || record.observed_version != format!("codex-cli {}", identity.version)
        || !record
            .observed_sha256
            .eq_ignore_ascii_case(&identity.sha256)
        || record.observed_path != identity.path
        || !record.synthetic_passed
        || !record.authenticated_passed
        || !record.explicitly_approved
    {
        return Err(AppServerError::CompatibilityRequired);
    }
    Ok(())
}

fn stable_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppServerError {
    StartupNotVerified,
    CompatibilityRequired,
    AuthenticationRequired,
    InvalidIdentity,
    ExecutableMissing,
    HashMismatch,
    #[allow(dead_code)]
    VersionMismatch,
    LaunchFailed,
    Timeout,
    OutputTooLarge,
    InvalidMessage,
    UnexpectedMessage,
    ProtocolError,
    InvalidQuota,
}

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    SetHandleInformation, BOOL, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::JobObjects::{
    CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTF_USESTDHANDLES,
    STARTUPINFOEXW,
};

fn launch_error<T>(result: windows::core::Result<T>) -> Result<T, AppServerError> {
    result.map_err(|_| AppServerError::LaunchFailed)
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

// Only called for successful Win32 APIs that transfer ownership.
fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}

fn pipe(child_reads: bool) -> Result<(File, OwnedHandle), AppServerError> {
    let attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: BOOL(1),
    };
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    launch_error(unsafe { CreatePipe(&mut read, &mut write, Some(&attrs), 4096) })?;
    let read = owned(read);
    let write = owned(write);
    let (parent, child) = if child_reads {
        (write, read)
    } else {
        (read, write)
    };
    launch_error(unsafe {
        SetHandleInformation(raw(&parent), HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))
    })?;
    Ok((File::from(parent), child))
}

struct AttributeList {
    // Pointer-sized storage provides alignment for the opaque Windows structure.
    _storage: Vec<usize>,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new() -> Result<Self, AppServerError> {
        let mut bytes = 0;
        let _ = unsafe {
            InitializeProcThreadAttributeList(
                LPPROC_THREAD_ATTRIBUTE_LIST::default(),
                2,
                0,
                &mut bytes,
            )
        };
        if bytes == 0 || bytes > 1024 * 1024 {
            return Err(AppServerError::LaunchFailed);
        }
        let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        launch_error(unsafe { InitializeProcThreadAttributeList(list, 2, 0, &mut bytes) })?;
        Ok(Self {
            _storage: storage,
            list,
        })
    }

    fn set_handles(&self, attribute: u32, handles: &[HANDLE]) -> Result<(), AppServerError> {
        launch_error(unsafe {
            UpdateProcThreadAttribute(
                self.list,
                0,
                attribute as usize,
                Some(handles.as_ptr().cast()),
                std::mem::size_of_val(handles),
                None,
                None,
            )
        })
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.list) };
    }
}

fn wide(value: &std::ffi::OsStr) -> Result<Vec<u16>, AppServerError> {
    let mut result: Vec<u16> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(AppServerError::InvalidIdentity);
    }
    result.push(0);
    Ok(result)
}

fn environment_block(isolated: Option<&Path>) -> Result<Vec<u16>, AppServerError> {
    let mut block = Vec::new();
    // Alphabetical, case-insensitive order as required by CreateProcessW.
    for name in [
        "APPDATA",
        "CODEX_HOME",
        "LOCALAPPDATA",
        "SystemRoot",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "WINDIR",
    ] {
        let value = match (isolated, name) {
            (Some(root), "CODEX_HOME") => Some(root.join("codex").into_os_string()),
            (Some(root), "APPDATA") => Some(root.join("roaming").into_os_string()),
            (Some(root), "LOCALAPPDATA") => Some(root.join("local").into_os_string()),
            (Some(root), "TEMP" | "TMP") => Some(root.join("temp").into_os_string()),
            (Some(root), "USERPROFILE") => Some(root.to_path_buf().into_os_string()),
            _ => std::env::var_os(name),
        };
        if let Some(value) = value {
            let mut entry = std::ffi::OsString::from(format!("{name}="));
            entry.push(value);
            block.extend(wide(&entry)?);
        }
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

struct ContainedChild {
    process: OwnedHandle,
    job: Option<OwnedHandle>,
}

impl ContainedChild {
    fn launch(
        executable: &Path,
        workdir: &Path,
        arguments: &str,
        isolated: Option<&Path>,
    ) -> Result<(Self, File, File, File), AppServerError> {
        let job = owned(launch_error(unsafe {
            CreateJobObjectW(None, PCWSTR::null())
        })?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        launch_error(unsafe {
            SetInformationJobObject(
                raw(&job),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        })?;
        let (stdin, child_stdin) = pipe(true)?;
        let (stdout, child_stdout) = pipe(false)?;
        let (stderr, child_stderr) = pipe(false)?;
        let handles = [raw(&child_stdin), raw(&child_stdout), raw(&child_stderr)];
        let jobs = [raw(&job)];
        let attributes = AttributeList::new()?;
        attributes.set_handles(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
        attributes.set_handles(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
        let application = wide(executable.as_os_str())?;
        let directory = wide(workdir.as_os_str())?;
        // Arguments are fixed internal literals, never drawn from user settings.
        let mut command = std::ffi::OsString::from("\"");
        command.push(executable);
        command.push(format!("\" {arguments}"));
        let mut command = wide(&command)?;
        let environment = environment_block(isolated)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = attributes.list;
        let mut info = PROCESS_INFORMATION::default();
        // The kernel assigns the job before any child code can execute.
        // Failure has no fallback to an uncontained process.
        launch_error(unsafe {
            CreateProcessW(
                PCWSTR(application.as_ptr()),
                PWSTR(command.as_mut_ptr()),
                None,
                None,
                BOOL(1),
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                Some(environment.as_ptr().cast()),
                PCWSTR(directory.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            )
        })?;
        let process = owned(info.hProcess);
        drop(owned(info.hThread));
        Ok((
            Self {
                process,
                job: Some(job),
            },
            stdin,
            stdout,
            stderr,
        ))
    }

    fn wait(&self, timeout: Duration) -> Result<bool, AppServerError> {
        match unsafe { WaitForSingleObject(raw(&self.process), timeout.as_millis() as u32) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(AppServerError::LaunchFailed),
        }
    }

    fn shutdown(&mut self, graceful: bool) -> Result<(), AppServerError> {
        if graceful && self.wait(SHUTDOWN_TIMEOUT)? {
            self.job.take();
            let mut code = 0;
            launch_error(unsafe { GetExitCodeProcess(raw(&self.process), &mut code) })?;
            return if code == 0 {
                Ok(())
            } else {
                Err(AppServerError::LaunchFailed)
            };
        }
        self.job.take();
        if !self.wait(SHUTDOWN_TIMEOUT)? {
            let _ = unsafe { TerminateProcess(raw(&self.process), 1) };
            if !self.wait(SHUTDOWN_TIMEOUT)? {
                return Err(AppServerError::Timeout);
            }
        }
        if graceful {
            Err(AppServerError::Timeout)
        } else {
            Ok(())
        }
    }
}

impl Drop for ContainedChild {
    fn drop(&mut self) {
        self.job.take();
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

pub fn sha256_file(path: &Path) -> Result<String, AppServerError> {
    let mut file = File::open(path).map_err(|_| AppServerError::ExecutableMissing)?;
    hash_reader(&mut file)
}

fn hash_reader(file: &mut impl Read) -> Result<String, AppServerError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
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
        || !stable_version(&identity.version)
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

// Version output is deliberately not logged, even on failure. The probe is a
// verification-test helper; production approval consumes externally produced
// evidence and never launches a version probe.
#[cfg(test)]
fn validate_version_output(output: &[u8], expected: &str) -> Result<(), AppServerError> {
    if !stable_version(expected) || output.len() > 256 {
        return Err(AppServerError::VersionMismatch);
    }
    let output = std::str::from_utf8(output).map_err(|_| AppServerError::VersionMismatch)?;
    let expected = format!("codex-cli {expected}");
    if output == expected
        || output == format!("{expected}\n")
        || output == format!("{expected}\r\n")
    {
        Ok(())
    } else {
        Err(AppServerError::VersionMismatch)
    }
}

// Only called with an already verified executable hash and a freshly created,
// dedicated synthetic profile. This is not an approval mechanism: callers
// still need independently established official release provenance.
#[cfg(test)]
fn verify_version_in_profile(
    identity: &ApprovedExecutable,
    root: &Path,
) -> Result<(), AppServerError> {
    let executable = validate_identity(identity)?;
    let mut locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&executable)
        .map_err(|_| AppServerError::ExecutableMissing)?;
    if !hash_reader(&mut locked)?.eq_ignore_ascii_case(&identity.sha256) {
        return Err(AppServerError::HashMismatch);
    }
    for name in ["codex", "roaming", "local", "temp"] {
        std::fs::create_dir(root.join(name)).map_err(|_| AppServerError::LaunchFailed)?;
    }
    let (mut child, stdin, stdout, stderr) =
        ContainedChild::launch(&executable, root, "--version", Some(root))?;
    drop(stdin);
    let result = collect_version_output(&mut child, stdout, stderr, &identity.version);
    drop(locked);
    result
}

#[cfg(test)]
fn collect_version_output(
    child: &mut ContainedChild,
    stdout: File,
    stderr: File,
    expected: &str,
) -> Result<(), AppServerError> {
    // Read only a bounded amount on each stream, even if a child floods output.
    let stdout_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout.take(257).read_to_end(&mut output)?;
        Ok::<_, std::io::Error>(output)
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        stderr.take(257).read_to_end(&mut output)?;
        Ok::<_, std::io::Error>(output.is_empty())
    });
    let shutdown = child.shutdown(true);
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while !stdout_thread.is_finished() || !stderr_thread.is_finished() {
        if Instant::now() >= deadline {
            return Err(AppServerError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = stdout_thread
        .join()
        .map_err(|_| AppServerError::LaunchFailed)?
        .map_err(|_| AppServerError::LaunchFailed)?;
    let stderr_empty = stderr_thread
        .join()
        .map_err(|_| AppServerError::LaunchFailed)?
        .map_err(|_| AppServerError::LaunchFailed)?;
    shutdown?;
    if !stderr_empty {
        return Err(AppServerError::VersionMismatch);
    }
    validate_version_output(&output, expected)
}

// An approval record is not transferable to another installation path.
// The external verifier must establish release provenance and startup safety;
// local version output alone is never sufficient to approve polling.
fn prepare_compatibility_record(
    identity: &ApprovedExecutable,
    evidence: CompatibilityRecord,
) -> Result<ApprovedExecutable, AppServerError> {
    let mut candidate = identity.clone();
    candidate.compatibility = Some(evidence);
    check_compatibility(&candidate)?;
    validate_identity(&candidate)?;
    Ok(candidate)
}

// Read-only UI status: never execute a candidate, read credentials, or treat
// a populated record as permission to lift the compiled launch freeze.
pub fn compatibility_status(identity: Option<&ApprovedExecutable>) -> &'static str {
    match identity {
        None => "No Codex executable has been selected. Live polling is disabled pending independent verification and explicit approval.",
        Some(identity) if check_compatibility(identity).is_err() => {
            "The configured executable lacks matching compatibility evidence. Live polling is disabled."
        }
        Some(_) => {
            "Compatibility evidence is recorded, but live polling remains disabled until runtime security acceptance is complete."
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceImport {
    identity: ApprovedExecutable,
    evidence: CompatibilityRecord,
}

pub fn inspect_evidence_file(path: &Path) -> Result<ApprovedExecutable, AppServerError> {
    let file = File::open(path).map_err(|_| AppServerError::InvalidIdentity)?;
    let metadata = file
        .metadata()
        .map_err(|_| AppServerError::InvalidIdentity)?;
    if !metadata.is_file() {
        return Err(AppServerError::InvalidIdentity);
    }
    if metadata.len() > 16 * 1024 {
        return Err(AppServerError::OutputTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppServerError::InvalidIdentity)?;
    if bytes.len() > 16 * 1024 {
        return Err(AppServerError::OutputTooLarge);
    }
    let import: EvidenceImport =
        serde_json::from_slice(&bytes).map_err(|_| AppServerError::InvalidIdentity)?;
    inspect_evidence(import)
}

fn inspect_evidence(import: EvidenceImport) -> Result<ApprovedExecutable, AppServerError> {
    // Evidence is a user-supplied attestation, not independently authenticated
    // provenance. Do not execute the file or accept pre-granted user consent.
    if import.identity.compatibility.is_some() || import.evidence.explicitly_approved {
        return Err(AppServerError::CompatibilityRequired);
    }
    let mut evidence = import.evidence;
    evidence.explicitly_approved = true;
    let mut candidate = prepare_compatibility_record(&import.identity, evidence)?;
    if let Some(record) = candidate.compatibility.as_mut() {
        record.explicitly_approved = false;
    }
    Ok(candidate)
}

pub fn confirm_evidence(
    candidate: ApprovedExecutable,
) -> Result<ApprovedExecutable, AppServerError> {
    let mut evidence = candidate
        .compatibility
        .clone()
        .ok_or(AppServerError::CompatibilityRequired)?;
    if evidence.explicitly_approved {
        return Err(AppServerError::CompatibilityRequired);
    }
    evidence.explicitly_approved = true;
    prepare_compatibility_record(&candidate, evidence)
}

fn require_startup_acceptance() -> Result<(), AppServerError> {
    Err(AppServerError::StartupNotVerified)
}

/// Retrieve quota data using only the fixed app-server request sequence.
pub fn read_rate_limits(identity: &ApprovedExecutable) -> Result<UsageData, AppServerError> {
    // Deliberately not configurable: no authenticated invocation has been accepted.
    require_startup_acceptance()?;
    check_compatibility(identity)?;
    let executable = validate_identity(identity)?;
    // Hold a read-only, no-write/no-delete sharing handle across process creation.
    let mut locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&executable)
        .map_err(|_| AppServerError::ExecutableMissing)?;
    if !hash_reader(&mut locked)?.eq_ignore_ascii_case(&identity.sha256) {
        return Err(AppServerError::HashMismatch);
    }
    let workdir = dirs::config_dir()
        .ok_or(AppServerError::LaunchFailed)?
        .join("CodexUsageHardened")
        .join("app-server-work");
    std::fs::create_dir_all(&workdir).map_err(|_| AppServerError::LaunchFailed)?;
    let (mut guard, mut stdin, stdout, stderr) =
        ContainedChild::launch(&executable, &workdir, "app-server --listen stdio://", None)?;

    drop(locked);
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
    send_message(
        writer,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"clientInfo": {"name": "codex-usage-monitor-hardened",
            "version": env!("CARGO_PKG_VERSION")}, "capabilities": {}}
        }),
    )?;
    let initialized = receive(1, Instant::now() + STARTUP_TIMEOUT)?;
    if !initialized.is_object() {
        return Err(AppServerError::ProtocolError);
    }
    send_message(
        writer,
        serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        }),
    )?;
    send_message(
        writer,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": {}
        }),
    )?;
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
                && !object.contains_key("result")
                && !object.contains_key("error") =>
            {
                continue
            }
            Some(value) if value.as_u64() == Some(id) && !object.contains_key("method") => {
                if let Some(error) = object.get("error") {
                    if id == 2
                        && !object.contains_key("result")
                        && error.get("code").and_then(|value| value.as_i64()) == Some(-32600)
                        && error.get("message").and_then(|value| value.as_str())
                            == Some("codex account authentication required to read rate limits")
                    {
                        return Err(AppServerError::AuthenticationRequired);
                    }
                    return Err(AppServerError::ProtocolError);
                }
                return object
                    .get("result")
                    .cloned()
                    .ok_or(AppServerError::ProtocolError);
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
        let available = reader
            .fill_buf()
            .map_err(|_| AppServerError::LaunchFailed)?;
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

    fn synthetic_arg(name: &str) -> Option<String> {
        std::env::args().skip_while(|arg| arg != name).nth(1)
    }

    // Development-only app-server stand-in. It is launched through the same
    // ContainedChild, bounded readers, exchange and shutdown path as the
    // production client, but it never reads credentials or contacts Codex.
    #[test]
    #[ignore = "synthetic app-server fixture; invoked by shared runner tests"]
    fn synthetic_app_server_fixture() {
        let mode = synthetic_arg("--scenario").unwrap_or_else(|| "clean".to_string());
        let marker = synthetic_arg("--marker");
        let port = synthetic_arg("--port").and_then(|value| value.parse::<u16>().ok());
        if let Some(path) = marker.as_ref() {
            if mode != "clean" {
                std::fs::write(path, format!("started:{mode}")).unwrap();
            }
        }
        if mode == "premature" {
            std::process::exit(23);
        }
        if mode == "stall" {
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode == "network" {
            if let Some(port) = port {
                let _ = std::net::TcpStream::connect(("127.0.0.1", port));
            }
        }
        if mode == "stdout-flood" {
            print!("{}", "x".repeat(MAX_TOTAL_OUTPUT_BYTES + 1));
            return;
        }
        if mode == "stderr-flood" {
            eprint!("{}", "x".repeat(MAX_TOTAL_OUTPUT_BYTES + 1));
            return;
        }
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap();
            let message: serde_json::Value = serde_json::from_str(&line).unwrap();
            match message.get("method").and_then(|value| value.as_str()) {
                Some("initialize") => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "jsonrpc":"2.0", "id": message["id"], "result": {}
                        })
                    );
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                }
                Some("initialized") => {}
                Some("account/rateLimits/read") => {
                    if mode == "malformed" {
                        println!("not-json");
                    } else {
                        println!(
                            "{}",
                            serde_json::json!({
                                "jsonrpc":"2.0", "id": message["id"], "error": {
                                    "code": -32600,
                                    "message": "codex account authentication required to read rate limits"
                                }
                            })
                        );
                    }
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                }
                _ => {}
            }
        }
    }

    #[derive(Debug)]
    struct SyntheticRun {
        result: Result<(), AppServerError>,
        marker_exists: bool,
        network_seen: bool,
        cleaned: bool,
    }

    // libtest may write its own human-readable status lines to the child
    // stdout. The stand-in runner filters only those known non-JSON lines;
    // production exchange remains strict and never performs this filtering.
    fn receive_synthetic_response(
        rx: &Receiver<Result<Vec<u8>, AppServerError>>,
        limit: &AtomicBool,
        id: u64,
        deadline: Instant,
    ) -> Result<serde_json::Value, AppServerError> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AppServerError::Timeout);
            }
            let line = rx.recv_timeout(remaining).map_err(|error| match error {
                std::sync::mpsc::RecvTimeoutError::Timeout => AppServerError::Timeout,
                std::sync::mpsc::RecvTimeoutError::Disconnected => AppServerError::LaunchFailed,
            })??;
            if limit.load(Ordering::Acquire) {
                return Err(AppServerError::OutputTooLarge);
            }
            let Ok(envelope) = serde_json::from_slice::<serde_json::Value>(&line) else {
                continue;
            };
            let Some(object) = envelope.as_object() else {
                return Err(AppServerError::InvalidMessage);
            };
            if object.get("id").and_then(|value| value.as_u64()) != Some(id) {
                continue;
            }
            if let Some(error) = object.get("error") {
                if id == 2 && error.get("code").and_then(|value| value.as_i64()) == Some(-32600) {
                    return Err(AppServerError::AuthenticationRequired);
                }
                return Err(AppServerError::ProtocolError);
            }
            return object
                .get("result")
                .cloned()
                .ok_or(AppServerError::InvalidMessage);
        }
    }

    fn run_synthetic_scenario(mode: &str) -> SyntheticRun {
        let root = std::env::temp_dir().join(format!(
            "codex-scenario-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("side-effect.marker");
        let listener = if mode == "network" {
            Some(std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap())
        } else {
            None
        };
        let port = listener
            .as_ref()
            .map(|socket| socket.local_addr().unwrap().port());
        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let args = format!(
            "--exact app_server::tests::synthetic_app_server_fixture --ignored --nocapture -- --scenario {mode} --marker {}{}",
            marker.display(),
            port.map_or(String::new(), |port| format!(" --port {port}"))
        );
        let launched = ContainedChild::launch(&executable, &root, &args, Some(&root));
        let (mut child, mut stdin, stdout, stderr) = match launched {
            Ok(value) => value,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&root);
                return SyntheticRun {
                    result: Err(error),
                    marker_exists: false,
                    network_seen: false,
                    cleaned: !root.exists(),
                };
            }
        };
        let (tx, rx) = sync_channel(16);
        let output_limit = Arc::new(AtomicBool::new(false));
        let stdout_thread = std::thread::spawn({
            let limit = Arc::clone(&output_limit);
            move || read_stdout(stdout, tx, limit)
        });
        let stderr_limit = Arc::clone(&output_limit);
        let stderr_thread = std::thread::spawn(move || drain_stderr(stderr, stderr_limit));
        let result = if mode == "mcp"
            || mode == "hook"
            || mode == "plugin"
            || mode == "premature"
            || mode == "stall"
            || mode == "stdout-flood"
            || mode == "stderr-flood"
            || mode == "malformed"
            || mode == "clean"
            || mode == "network"
        {
            exchange(&mut stdin, |id, deadline| {
                receive_synthetic_response(&rx, &output_limit, id, deadline)
            })
            .map(|_| ())
        } else {
            Err(AppServerError::InvalidIdentity)
        };
        drop(stdin);
        drop(rx);
        let _ = child.shutdown(result.is_ok());
        let _ = finish_readers([stdout_thread, stderr_thread]);
        let network_seen = listener
            .as_ref()
            .map(|socket| socket.set_nonblocking(true).is_ok() && socket.accept().is_ok())
            .unwrap_or(false);
        let marker_exists = marker.exists();
        let _ = child.shutdown(false);
        drop(listener);
        let _ = std::fs::remove_dir_all(&root);
        SyntheticRun {
            result,
            marker_exists,
            network_seen,
            cleaned: !root.exists(),
        }
    }

    #[test]
    fn shared_runner_correlation_matrix_detects_permitted_and_prohibited_activity() {
        let clean = run_synthetic_scenario("clean");
        assert_eq!(
            clean.result,
            Err(AppServerError::AuthenticationRequired),
            "clean={clean:?}"
        );
        assert!(!clean.marker_exists && clean.cleaned);

        for mode in ["mcp", "hook", "plugin"] {
            let run = run_synthetic_scenario(mode);
            assert_eq!(run.result, Err(AppServerError::AuthenticationRequired));
            assert!(run.marker_exists && run.cleaned, "{mode}");
        }
        let network = run_synthetic_scenario("network");
        assert_eq!(network.result, Err(AppServerError::AuthenticationRequired));
        assert!(network.network_seen && network.cleaned);
    }

    #[test]
    fn shared_runner_rejects_malformed_excessive_stalled_and_premature_servers() {
        for mode in [
            "malformed",
            "stdout-flood",
            "stderr-flood",
            "stall",
            "premature",
        ] {
            let run = run_synthetic_scenario(mode);
            assert!(run.result.is_err(), "{mode} unexpectedly succeeded");
            assert!(run.cleaned, "{mode} leaked its scenario directory");
        }
    }

    /// Explicitly invoked, test-only external verification entry point.
    ///
    /// This is intentionally not reachable from the production binary and does
    /// not consult compatibility approval or the production launch function.
    /// The caller must supply the executable path, expected hash, and expected
    /// stable version; no discovery or implicit trust is performed.
    #[test]
    #[ignore = "requires explicit external executable authorization"]
    fn externally_selected_codex_unauthenticated_exchange() {
        let executable = std::env::var_os("CODEX_VERIFY_EXECUTABLE")
            .expect("CODEX_VERIFY_EXECUTABLE must be explicitly supplied");
        let expected_hash = std::env::var("CODEX_VERIFY_SHA256")
            .expect("CODEX_VERIFY_SHA256 must be explicitly supplied");
        let expected_version = std::env::var("CODEX_VERIFY_VERSION")
            .expect("CODEX_VERIFY_VERSION must be explicitly supplied");
        assert!(
            stable_version(&expected_version),
            "unstable version supplied"
        );
        let selected_path = std::fs::canonicalize(PathBuf::from(executable))
            .expect("selected executable canonical path");
        let identity = ApprovedExecutable {
            path: selected_path,
            sha256: expected_hash,
            version: expected_version,
            compatibility: None,
        };
        let executable = validate_identity(&identity).expect("selected executable identity");
        let actual_hash = sha256_file(&executable).expect("selected executable hash");
        assert_eq!(
            actual_hash.to_ascii_lowercase(),
            identity.sha256.to_ascii_lowercase(),
            "selected executable changed before launch"
        );

        let root = std::env::temp_dir().join(format!(
            "codex-external-verification-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("verification root");
        for directory in ["codex", "roaming", "local", "temp"] {
            std::fs::create_dir_all(root.join(directory)).expect("synthetic profile directory");
        }
        let result = {
            let (mut child, mut stdin, stdout, stderr) = ContainedChild::launch(
                &executable,
                &root,
                "app-server --listen stdio://",
                Some(&root),
            )
            .expect("contained app-server launch");
            let (tx, rx) = sync_channel(16);
            let output_limit = Arc::new(AtomicBool::new(false));
            let stdout_thread = std::thread::spawn({
                let limit = Arc::clone(&output_limit);
                move || read_stdout(stdout, tx, limit)
            });
            let stderr_limit = Arc::clone(&output_limit);
            let stderr_thread = std::thread::spawn(move || drain_stderr(stderr, stderr_limit));
            let exchange_result = exchange(&mut stdin, |id, deadline| {
                receive_response(&rx, &output_limit, id, deadline)
            });
            drop(stdin);
            drop(rx);
            let shutdown_result = child.shutdown(exchange_result.is_ok());
            let readers_result = finish_readers([stdout_thread, stderr_thread]);
            (exchange_result, shutdown_result, readers_result)
        };
        let cleanup_result = std::fs::remove_dir_all(&root);
        assert!(!root.exists(), "verification root was not removed");
        cleanup_result.expect("verification root cleanup");
        let (exchange_result, shutdown_result, readers_result) = result;
        if !matches!(exchange_result, Err(AppServerError::AuthenticationRequired)) {
            panic!(
                "expected the sanitized unauthenticated rate-limit response; category={:?}",
                exchange_result.err()
            );
        }
        shutdown_result.expect("bounded app-server shutdown");
        readers_result.expect("stdout/stderr reader completion");
        println!(
            "EXTERNAL_CODEX_VERIFICATION_OK version={} hash={}",
            identity.version, identity.sha256
        );
    }

    // The fixture uses this test executable, not Codex. It must only be run
    // explicitly by the two parent tests below.
    #[test]
    #[ignore = "child-process fixture; invoked by containment tests"]
    fn contained_fixture() {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input).unwrap();
        assert_eq!(input, b"fixture\n");
        // Attempt a harmless descendant and require the job to reject it.
        let attempted = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .spawn();
        if let Ok(mut escaped) = attempted {
            let _ = escaped.kill();
            let _ = escaped.wait();
            panic!("contained child created a descendant");
        }
        println!("CONTAINED_FIXTURE_OK");
    }

    fn launch_fixture() -> (ContainedChild, File, File, File) {
        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let directory = executable.parent().unwrap();
        ContainedChild::launch(
            &executable,
            directory,
            "--exact app_server::tests::contained_fixture --ignored --nocapture",
            None,
        )
        .expect("creation-time job and pipe setup must succeed")
    }

    #[test]
    fn contained_child_closes_stdin_and_cannot_create_descendants() {
        let (mut child, mut stdin, stdout, stderr) = launch_fixture();
        stdin.write_all(b"fixture\n").unwrap();
        drop(stdin);
        child.shutdown(true).unwrap();
        let mut output = String::new();
        stdout.take(8192).read_to_string(&mut output).unwrap();
        let mut errors = String::new();
        stderr.take(8192).read_to_string(&mut errors).unwrap();
        assert!(output.contains("CONTAINED_FIXTURE_OK"), "{output} {errors}");
    }

    #[test]
    fn contained_child_is_terminated_when_stdin_remains_open() {
        let (mut child, stdin, _stdout, _stderr) = launch_fixture();
        assert!(!child.wait(Duration::from_millis(50)).unwrap());
        child.shutdown(false).unwrap();
        assert!(child.wait(Duration::ZERO).unwrap());
        drop(stdin);
    }

    #[test]
    fn evidence_import_requires_explicit_consent_and_revalidates_hash() {
        let path = std::env::temp_dir().join(format!("codex-evidence-{}", std::process::id()));
        std::fs::write(&path, b"synthetic evidence fixture").unwrap();
        let identity = ApprovedExecutable {
            path: path.canonicalize().unwrap(),
            sha256: sha256_file(&path).unwrap(),
            version: CODEX_VERSION.to_string(),
            compatibility: None,
        };
        let evidence = CompatibilityRecord {
            schema: COMPATIBILITY_SCHEMA,
            invocation_policy: INVOCATION_POLICY.to_string(),
            release_tag: format!("rust-v{}", identity.version),
            release_commit: "b".repeat(40),
            observed_version: format!("codex-cli {}", identity.version),
            observed_sha256: identity.sha256.clone(),
            observed_path: identity.path.clone(),
            synthetic_passed: true,
            authenticated_passed: true,
            explicitly_approved: false,
        };
        let mut preapproved = evidence.clone();
        preapproved.explicitly_approved = true;
        assert!(matches!(
            inspect_evidence(EvidenceImport {
                identity: identity.clone(),
                evidence: preapproved,
            }),
            Err(AppServerError::CompatibilityRequired)
        ));
        let candidate = inspect_evidence(EvidenceImport { identity, evidence }).unwrap();
        assert_eq!(
            check_compatibility(&candidate),
            Err(AppServerError::CompatibilityRequired)
        );
        let approved = confirm_evidence(candidate.clone()).unwrap();
        assert_eq!(check_compatibility(&approved), Ok(()));
        assert!(matches!(
            read_rate_limits(&approved),
            Err(AppServerError::StartupNotVerified)
        ));
        std::fs::write(&path, b"changed synthetic fixture").unwrap();
        assert!(matches!(
            confirm_evidence(candidate),
            Err(AppServerError::HashMismatch)
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn evidence_import_rejects_oversized_and_unknown_fields() {
        let path = std::env::temp_dir().join(format!("codex-evidence-size-{}", std::process::id()));
        std::fs::write(&path, vec![b' '; 16 * 1024 + 1]).unwrap();
        assert!(matches!(
            inspect_evidence_file(&path),
            Err(AppServerError::OutputTooLarge)
        ));
        std::fs::write(&path, br#"{"identity":{"path":"fixture","sha256":"x","version":"0.172.0","unknown":true},"evidence":{}}"#).unwrap();
        assert!(matches!(
            inspect_evidence_file(&path),
            Err(AppServerError::InvalidIdentity)
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn compatibility_requires_matching_version_hash_and_completed_approval() {
        let mut identity = ApprovedExecutable {
            path: PathBuf::from(r"C:\fixture\codex.exe"),
            sha256: "A".repeat(64),
            version: "0.173.0".to_string(),
            compatibility: None,
        };
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
        identity.compatibility = Some(CompatibilityRecord {
            schema: COMPATIBILITY_SCHEMA,
            invocation_policy: INVOCATION_POLICY.to_string(),
            release_tag: "rust-v0.173.0".to_string(),
            release_commit: "b".repeat(40),
            observed_version: "codex-cli 0.173.0".to_string(),
            observed_sha256: identity.sha256.clone(),
            observed_path: identity.path.clone(),
            synthetic_passed: true,
            authenticated_passed: true,
            explicitly_approved: true,
        });
        assert_eq!(check_compatibility(&identity), Ok(()));
        // Even a fully populated synthetic record cannot lift the current
        // compiled-in launch freeze.
        assert!(matches!(
            read_rate_limits(&identity),
            Err(AppServerError::StartupNotVerified)
        ));
        identity.sha256 = "C".repeat(64);
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
        identity.sha256 = "A".repeat(64);
        identity
            .compatibility
            .as_mut()
            .unwrap()
            .authenticated_passed = false;
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
    }

    #[test]
    fn compatibility_cannot_be_transferred_to_another_path_or_policy() {
        let mut identity = ApprovedExecutable {
            path: PathBuf::from(r"C:\fixture\codex.exe"),
            sha256: "A".repeat(64),
            version: CODEX_VERSION.to_string(),
            compatibility: None,
        };
        let evidence = CompatibilityRecord {
            schema: COMPATIBILITY_SCHEMA,
            invocation_policy: INVOCATION_POLICY.to_string(),
            release_tag: format!("rust-v{}", identity.version),
            release_commit: "b".repeat(40),
            observed_version: format!("codex-cli {}", identity.version),
            observed_sha256: identity.sha256.clone(),
            observed_path: identity.path.clone(),
            synthetic_passed: true,
            authenticated_passed: true,
            explicitly_approved: true,
        };
        identity.compatibility = Some(evidence.clone());
        assert_eq!(check_compatibility(&identity), Ok(()));
        identity.path = PathBuf::from(r"C:\other\codex.exe");
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
        identity.path = evidence.observed_path.clone();
        let record = identity.compatibility.as_mut().unwrap();
        record.schema = 1;
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
        let record = identity.compatibility.as_mut().unwrap();
        record.schema = COMPATIBILITY_SCHEMA;
        record.invocation_policy = "different-policy".to_string();
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
        let mut relative = identity.clone();
        relative.path = PathBuf::from("codex.exe");
        let mut relative_evidence = evidence;
        relative_evidence.observed_path = relative.path.clone();
        assert!(matches!(
            prepare_compatibility_record(&relative, relative_evidence),
            Err(AppServerError::InvalidIdentity)
        ));
    }

    #[test]
    fn legacy_identity_deserializes_without_granting_approval() {
        let identity: ApprovedExecutable = serde_json::from_value(serde_json::json!({
            "path": "C:\\fixture\\codex.exe",
            "version": "0.172.0",
            "sha256": "A".repeat(64)
        }))
        .unwrap();
        assert!(identity.compatibility.is_none());
        assert_eq!(
            check_compatibility(&identity),
            Err(AppServerError::CompatibilityRequired)
        );
    }

    #[test]
    #[ignore = "synthetic environment fixture; invoked by isolation test"]
    fn isolated_profile_fixture() {
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        for (name, relative) in [
            ("CODEX_HOME", "codex"),
            ("APPDATA", "roaming"),
            ("LOCALAPPDATA", "local"),
            ("TEMP", "temp"),
            ("TMP", "temp"),
        ] {
            let actual = PathBuf::from(std::env::var_os(name).unwrap());
            assert_eq!(actual.canonicalize().unwrap(), root.join(relative));
        }
        let profile = PathBuf::from(std::env::var_os("USERPROFILE").unwrap());
        assert_eq!(profile.canonicalize().unwrap(), root);
        assert!(std::env::var_os("OPENAI_API_KEY").is_none());
        assert!(std::env::var_os("PATH").is_none());
        println!("ISOLATED_PROFILE_OK");
    }

    struct FixtureProfile(PathBuf);

    impl FixtureProfile {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "codex-hardened-profile-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }

    impl Drop for FixtureProfile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn version_probe_rejects_non_codex_executable_output() {
        let profile = FixtureProfile::new();
        let path = std::env::current_exe().unwrap().canonicalize().unwrap();
        let identity = ApprovedExecutable {
            sha256: sha256_file(&path).unwrap(),
            path,
            version: CODEX_VERSION.to_string(),
            compatibility: None,
        };
        // The Rust test binary is trusted as a fixture but does not implement
        // Codex's version command. Never interpret its output as approval.
        assert!(verify_version_in_profile(&identity, &profile.0).is_err());
    }

    #[test]
    fn synthetic_profile_is_passed_to_contained_child() {
        let profile = FixtureProfile::new();
        for name in ["codex", "roaming", "local", "temp"] {
            std::fs::create_dir(profile.0.join(name)).unwrap();
        }
        let path = std::env::current_exe().unwrap().canonicalize().unwrap();
        let (mut child, stdin, stdout, _stderr) = ContainedChild::launch(
            &path,
            &profile.0,
            "--exact app_server::tests::isolated_profile_fixture --ignored --nocapture",
            Some(&profile.0),
        )
        .unwrap();
        drop(stdin);
        child.shutdown(true).unwrap();
        let mut output = String::new();
        stdout.take(8192).read_to_string(&mut output).unwrap();
        assert!(output.contains("ISOLATED_PROFILE_OK"));
    }

    #[test]
    fn version_output_accepts_only_exact_stable_cli_version() {
        for output in [
            b"codex-cli 0.172.0".as_slice(),
            b"codex-cli 0.172.0\n",
            b"codex-cli 0.172.0\r\n",
        ] {
            assert_eq!(validate_version_output(output, "0.172.0"), Ok(()));
        }
        for output in [
            b"codex-cli 0.173.0\n".as_slice(),
            b"codex-cli 0.172.0-alpha.1\n",
            b"codex-cli 0.172.0\nunexpected",
            b" codex-cli 0.172.0\n",
            b"\xff",
        ] {
            assert_eq!(
                validate_version_output(output, "0.172.0"),
                Err(AppServerError::VersionMismatch)
            );
        }
        assert_eq!(
            validate_version_output(&[b'x'; 257], "0.172.0"),
            Err(AppServerError::VersionMismatch)
        );
    }

    #[test]
    fn unverified_startup_cannot_launch_even_with_configured_identity() {
        let identity = ApprovedExecutable {
            path: PathBuf::from("not-an-executable"),
            sha256: "0".repeat(64),
            version: CODEX_VERSION.to_string(),
            compatibility: None,
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
    fn frame_limit_accepts_exact_boundary_and_rejects_one_byte_over() {
        let mut exact =
            std::io::Cursor::new([vec![b'x'; MAX_JSON_LINE_BYTES - 1], vec![b'\n']].concat());
        assert_eq!(
            read_frame(&mut exact).unwrap().unwrap().len(),
            MAX_JSON_LINE_BYTES
        );
        let mut over = std::io::Cursor::new(vec![b'x'; MAX_JSON_LINE_BYTES + 1]);
        assert_eq!(read_frame(&mut over), Err(AppServerError::OutputTooLarge));
    }

    #[test]
    fn stalled_response_deadline_is_bounded() {
        let (_tx, rx) = sync_channel(1);
        let started = Instant::now();
        assert_eq!(
            receive_response(
                &rx,
                &AtomicBool::new(false),
                1,
                started + Duration::from_millis(20)
            ),
            Err(AppServerError::Timeout)
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn stdout_queue_and_total_floods_set_output_limit() {
        let (tx, rx) = sync_channel(1);
        tx.send(Ok(b"already queued\n".to_vec())).unwrap();
        let limit = Arc::new(AtomicBool::new(false));
        read_stdout(
            std::io::Cursor::new(b"second\nthird\n".to_vec()),
            tx,
            Arc::clone(&limit),
        );
        assert!(limit.load(Ordering::Acquire));
        drop(rx);

        let (tx, _rx) = sync_channel(16);
        let limit = Arc::new(AtomicBool::new(false));
        let flood = vec![b'x'; MAX_TOTAL_OUTPUT_BYTES + 1];
        read_stdout(std::io::Cursor::new(flood), tx, Arc::clone(&limit));
        assert!(limit.load(Ordering::Acquire));
    }

    #[test]
    fn stderr_flood_sets_output_limit_without_blocking() {
        let limit = Arc::new(AtomicBool::new(false));
        let flood = vec![b'x'; MAX_TOTAL_OUTPUT_BYTES + 1];
        drain_stderr(std::io::Cursor::new(flood), Arc::clone(&limit));
        assert!(limit.load(Ordering::Acquire));
    }

    #[test]
    fn invalid_json_encoding_is_rejected_after_bounded_frame_read() {
        let (tx, rx) = sync_channel(1);
        tx.send(Ok(vec![0xff, b'\n'])).unwrap();
        assert_eq!(
            receive_response(
                &rx,
                &AtomicBool::new(false),
                1,
                Instant::now() + STARTUP_TIMEOUT
            ),
            Err(AppServerError::InvalidMessage)
        );
    }

    #[test]
    fn unexpected_response_id_is_rejected() {
        let (tx, rx) = sync_channel(1);
        tx.send(Ok(br#"{"id":2,"result":{}}"#.to_vec())).unwrap();
        assert_eq!(
            receive_response(
                &rx,
                &AtomicBool::new(false),
                1,
                Instant::now() + STARTUP_TIMEOUT
            ),
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
            compatibility: None,
        };
        assert_eq!(
            validate_identity(&identity).unwrap(),
            path.canonicalize().unwrap()
        );

        let mut changed = identity.clone();
        let replacement = if changed.sha256.starts_with('0') {
            "1"
        } else {
            "0"
        };
        changed.sha256.replace_range(..1, replacement);
        assert_eq!(
            validate_identity(&changed),
            Err(AppServerError::HashMismatch)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn identity_rejects_relative_paths_and_prerelease_versions() {
        let relative = ApprovedExecutable {
            path: PathBuf::from("codex.exe"),
            sha256: "0".repeat(64),
            version: CODEX_VERSION.to_string(),
            compatibility: None,
        };
        assert_eq!(
            validate_identity(&relative),
            Err(AppServerError::InvalidIdentity)
        );

        let wrong_version = ApprovedExecutable {
            path: PathBuf::from(r"C:\codex.exe"),
            sha256: "0".repeat(64),
            version: "0.167.0-alpha.1".to_string(),
            compatibility: None,
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
