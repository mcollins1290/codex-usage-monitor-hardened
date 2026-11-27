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
| T2–T6 synthetic fixture/configuration scenarios | `verification/Verify-Fixtures.ps1` | Implemented, self-tested | Official Codex execution remains pending |
| T7 normal/premature/timeout lifecycle controls | `verification/Verify-Fixtures.ps1` | Executed repeatedly | 10/10 complete synthetic runs passed |
| T8 process/filesystem/network positive controls | `verification/Verify-Fixtures.ps1` | Executed repeatedly | 10/10 complete synthetic runs passed |
| O40 deterministic fixture controls | `verification/Verify-Fixtures.ps1` | Executed repeatedly | Passed in 10/10 runs |
| O41 cleanup controls | `verification/Verify-Fixtures.ps1` | Executed repeatedly | Passed in 10/10 runs |
| Ignored child fixtures | `src/app_server.rs` tests | Executed indirectly | Four ignored tests are invoked by parent containment/isolation tests |
