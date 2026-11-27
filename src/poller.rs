//! Codex-only quota adapter.

use crate::app_server;
use crate::localization::Strings;
use crate::models::{QuotaDisplayMode, UsageData, UsageSection};
use std::time::{Duration, SystemTime};

pub const GEMINI_QUOTA_GROUP: &str = "Codex";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollError {
    AuthRequired,
    NoCredentials,
    TokenExpired,
    NetworkUnavailable,
    RateLimited(Option<u32>),
    ServerError,
    RequestFailed,
    ApprovalRequired,
    ExecutableChanged,
}
impl PollError {
    pub fn category(self) -> &'static str {
        match self {
            Self::AuthRequired => "auth_required",
            Self::NoCredentials => "credentials_unavailable",
            Self::TokenExpired => "authentication_unavailable",
            Self::NetworkUnavailable => "network_unavailable",
            Self::RateLimited(_) => "rate_limited",
            Self::ServerError => "server_error",
            Self::RequestFailed => "protocol_error",
            Self::ApprovalRequired => "approval_required",
            Self::ExecutableChanged => "executable_changed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialWatchMode {
    ActiveSource,
    AllSources,
    Antigravity,
    Codex,
}
pub type CredentialWatchSnapshot = Vec<String>;

pub fn poll_provider(
    id: usize,
    _group: Option<&str>,
    identity: Option<&app_server::ApprovedExecutable>,
) -> Result<UsageData, PollError> {
    if id != 1 {
        return Err(PollError::RequestFailed);
    }
    let identity = identity.ok_or(PollError::ApprovalRequired)?;
    app_server::read_rate_limits(identity).map_err(app_server_error)
}

pub fn format_quota_line(
    section: &UsageSection,
    strings: Strings,
    mode: QuotaDisplayMode,
    now: SystemTime,
) -> String {
    if !section.has_reading() {
        return "--".to_string();
    }
    let pct = format_percent(mode.percentage(section.percentage));
    let Some(reset) = section.resets_at else {
        return pct;
    };
    let countdown = match reset.duration_since(now) {
        Ok(remaining) => format_countdown_from_secs(remaining.as_secs(), strings),
        Err(_) => strings.now.to_string(),
    };
    format!("{pct} · {countdown}")
}
pub fn format_percent(value: f64) -> String {
    let value = if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    };
    if value > 0.0 && value < 1.0 {
        "<1%".to_string()
    } else if value > 99.0 && value < 100.0 {
        "99%".to_string()
    } else {
        format!("{:.0}%", value.round_ties_even())
    }
}
pub fn remaining_percentage(used_percentage: f64) -> f64 {
    (100.0 - used_percentage).clamp(0.0, 100.0)
}
pub fn time_until_display_change(resets_at: Option<SystemTime>) -> Option<Duration> {
    let remaining = resets_at?.duration_since(SystemTime::now()).ok()?;
    let seconds = remaining.as_secs();
    Some(Duration::from_secs(if seconds >= 86_400 {
        seconds / 86_400 * 86_400
    } else if seconds >= 3_600 {
        seconds / 3_600 * 3_600
    } else if seconds >= 60 {
        seconds / 60 * 60
    } else {
        seconds
    }))
}
fn format_countdown_from_secs(total_secs: u64, strings: Strings) -> String {
    let mins = total_secs / 60;
    let hours = total_secs / 3600;
    let days = total_secs / 86400;
    if days >= 1 {
        format!("{days}{}", strings.day_suffix)
    } else if hours >= 1 {
        format!("{hours}{}", strings.hour_suffix)
    } else if mins >= 1 {
        format!("{mins}{}", strings.minute_suffix)
    } else {
        format!("{total_secs}{}", strings.second_suffix)
    }
}
pub fn app_server_error(error: app_server::AppServerError) -> PollError {
    use app_server::AppServerError;
    match error {
        AppServerError::StartupNotVerified | AppServerError::CompatibilityRequired => {
            PollError::ApprovalRequired
        }
        AppServerError::InvalidIdentity
        | AppServerError::ExecutableMissing
        | AppServerError::HashMismatch
        | AppServerError::VersionMismatch => PollError::ExecutableChanged,
        AppServerError::AuthenticationRequired => PollError::AuthRequired,
        _ => PollError::RequestFailed,
    }
}

