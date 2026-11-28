# Verification command register

| Command | Exit code | Result |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo test --locked` | 0 | 93 passed, 0 failed, 5 ignored |
| `cargo test --locked app_server::tests::shared_runner` | 0 | 2 passed, 0 failed; synthetic stand-in correlation matrix |
| `cargo test --locked app_server::tests::externally_selected_codex_unauthenticated_exchange -- --ignored --nocapture` | 0 | Explicitly selected CLI 0.172.0 passed T1 with the expected sanitized authentication-required result; hash/path/version supplied out-of-band and not retained here |
| Ten consecutive shared Rust correlation runs | 0 each | 10/10 passed; elapsed milliseconds: 11916, 10259, 10243, 10231, 10234, 10231, 10226, 10231, 10225, 10207 |
| `cargo clippy --locked --all-targets` | 0 | No warnings |
| `cargo build --release --locked` | 0 | Optimized release build passed |
| `cargo audit --json` | 0 | 0 vulnerabilities; advisory DB `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; duplicate `syn` warning |
| PowerShell JSON parse of `docs/verification/checklist-dispositions.json` | 0 | 524 item records, 524 unique identifiers |
| `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File verification/Verify-Fixtures.ps1` | 0 | Synthetic suite passed; official Codex was not executed |
| PowerShell parser checks for both verification scripts | 0 | Both scripts parsed without syntax errors |
| Ten repeated synthetic fixture runs | 0 each | 10/10 passed; elapsed milliseconds: 3080, 3214, 2987, 3017, 3015, 3034, 3071, 3140, 3148, 3101 |
