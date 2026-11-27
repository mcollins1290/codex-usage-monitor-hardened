# Verification command register

| Command | Exit code | Result |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo test --locked` | 0 | 86 passed, 0 failed, 4 ignored |
| `cargo clippy --locked --all-targets` | 0 | No warnings |
| `cargo build --release --locked` | 0 | Optimized release build passed |
| `cargo audit --json` | 0 | 0 vulnerabilities; advisory DB `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; duplicate `syn` warning |
| PowerShell JSON parse of `docs/verification/checklist-dispositions.json` | 0 | 524 item records, 524 unique identifiers |
