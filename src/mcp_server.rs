//! Disabled compatibility surface during the Codex-only refactor.
//!
//! No socket is opened and no MCP request is accepted. The inherited UI is
//! removed in a later cleanup; this shim keeps the intermediate tree buildable.

use crate::provider_poll::Service;

pub const DEFAULT_PORT: u16 = 46827;
pub type Snapshot = Option<[ProviderRecord; 2]>;
#[derive(Clone)]
pub struct ProviderRecord;
impl ProviderRecord {
    pub fn from_service(_service: &Service) -> Self {
        Self
    }
}
pub fn start(_port: u16, _snapshot: fn() -> Snapshot) -> Result<(), String> {
    Err("MCP disabled".into())
}
pub fn stop() {}
