//! Disabled compatibility surface during the Codex-only refactor.
//!
//! This module deliberately performs no network, download, replacement, or
//! process-launch operation. It exists only until the inherited settings UI
//! update controls are removed completely.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallChannel {
    Portable,
    Winget,
}

#[derive(Clone, Debug)]
pub struct ReleaseDescriptor {
    pub latest_version: String,
}

pub enum UpdateCheckResult {
    UpToDate,
    Available(ReleaseDescriptor),
}

pub fn current_install_channel() -> InstallChannel {
    InstallChannel::Portable
}
pub fn handle_cli_mode(_args: &[String]) -> Option<i32> {
    None
}
pub fn check_for_updates() -> Result<UpdateCheckResult, String> {
    Ok(UpdateCheckResult::UpToDate)
}
pub fn begin_self_update(_release: &ReleaseDescriptor) -> Result<(), String> {
    Err("updates disabled".into())
}
pub fn begin_winget_update() -> Result<(), String> {
    Err("updates disabled".into())
}
