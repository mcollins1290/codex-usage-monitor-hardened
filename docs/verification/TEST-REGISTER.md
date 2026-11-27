# Test register

| Test or command | Source | Status | Result |
| --- | --- | --- | --- |
| `cargo test --locked` | Rust unit and containment tests | Executed | 86 passed, 0 failed, 4 ignored |
| `cargo fmt --all -- --check` | Full Rust source tree | Executed | Passed |
| `cargo clippy --locked --all-targets` | Full Rust source tree | Executed | Passed without warnings |
| `cargo build --release --locked` | Release target | Executed | Passed |
| `cargo audit --json` | Final `Cargo.lock` | Executed | 0 vulnerabilities |
| `cargo deny check` | Final dependency graph | Executed | Advisories, bans, licenses and sources passed; duplicate `syn` warning |
| T1 synthetic app-server harness | `verification/Verify-AppServer.ps1` | Implemented, not executed | Requires separately authorized synthetic runtime execution |
| T2–T8 startup-side-effect scenarios | `docs/RUNTIME-ACCEPTANCE.md` | Not implemented | Must not be represented as passed |
| Ignored child fixtures | `src/app_server.rs` tests | Executed indirectly | Four ignored tests are invoked by parent containment/isolation tests |
