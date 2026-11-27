# Dependency security review

Status: automated RustSec and cargo-deny checks completed for the current lockfile; zero vulnerabilities/advisories, bans and source-policy violations. Cargo-deny reports one duplicate `syn` version as a warning.

Baseline lockfile: 34bff340d9203807e7ad7a86e93302d82a6d7be7. 50 external package/version entries (including platform-specific and build dependencies).

| Crate | Locked version |
| --- | --- |
| block-buffer | 0.10.4 |
| cfg-if | 1.0.4 |
| cpufeatures | 0.2.17 |
| crypto-common | 0.1.7 |
| digest | 0.10.7 |
| dirs | 6.0.0 |
| dirs-sys | 0.5.0 |
| generic-array | 0.14.7 |
| getrandom | 0.2.17 |
| itoa | 1.0.17 |
| libc | 0.2.189 |
| libredox | 0.1.20 |
| memchr | 2.8.0 |
| option-ext | 0.2.0 |
| proc-macro2 | 1.0.106 |
| quote | 1.0.44 |
| redox_users | 0.5.2 |
| serde | 1.0.228 |
| serde_core | 1.0.228 |
| serde_derive | 1.0.228 |
| serde_json | 1.0.149 |
| sha2 | 0.10.9 |
| syn | 2.0.117 |
| syn | 3.0.4 |
| thiserror | 2.0.20 |
| thiserror-impl | 2.0.20 |
| toml | 0.5.11 |
| typenum | 1.20.1 |
| unicode-ident | 1.0.24 |
| version_check | 0.9.5 |
| wasi | 0.11.1+wasi-snapshot-preview1 |
| windows | 0.58.0 |
| windows-core | 0.58.0 |
| windows-implement | 0.58.0 |
| windows-interface | 0.58.0 |
| windows-link | 0.2.1 |
| windows-result | 0.2.0 |
| windows-strings | 0.1.0 |
| windows-sys | 0.61.2 |
| windows-targets | 0.52.6 |
| windows_aarch64_gnullvm | 0.52.6 |
| windows_aarch64_msvc | 0.52.6 |
| windows_i686_gnu | 0.52.6 |
| windows_i686_gnullvm | 0.52.6 |
| windows_i686_msvc | 0.52.6 |
| windows_x86_64_gnu | 0.52.6 |
| windows_x86_64_gnullvm | 0.52.6 |
| windows_x86_64_msvc | 0.52.6 |
| winres | 0.1.12 |
| zmij | 1.0.21 |

## Manual advisory review — 2026-11-26

RustSec advisory database revision: [c9f6c91a44dbad9d71742329e9df061279f9c4be](https://github.com/RustSec/advisory-db/tree/c9f6c91a44dbad9d71742329e9df061279f9c4be). Retrieved the complete recursive tree (2,632 entries; not truncated), intersected package directory names with the locked package names, and examined all four matching advisory records at that same revision.

| Crate | Advisory | Assessment |
| --- | --- | --- |
| dirs 6.0.0 | [RUSTSEC-2020-0053](https://rustsec.org/advisories/RUSTSEC-2020-0053.html) | Withdrawn 2021-04-19 |
| generic-array 0.14.7 | [RUSTSEC-2020-0146](https://rustsec.org/advisories/RUSTSEC-2020-0146.html) | Patched >=0.13.3 |
| sha2 0.10.9 | [RUSTSEC-2021-0100](https://rustsec.org/advisories/RUSTSEC-2021-0100.html) | Patched >=0.9.8 |
| windows 0.58.0 | [RUSTSEC-2022-0008](https://rustsec.org/advisories/RUSTSEC-2022-0008.html) | Patched >=0.32.0 |

No applicable advisories identified in this snapshot. This is not an execution of cargo-audit, does not check registry yanks, and does not establish absence of unknown vulnerabilities or supply-chain compromise. Recheck the final lockfile and fresh advisory database before acceptance.

## Final verification procedure

- Review current RustSec advisory database at a recorded commit, including vulnerability, unsoundness, unmaintained and yanked-package findings. Use a reviewed, version-pinned cargo-audit installation in an isolated verification environment, not an unreviewed installer on the user's PC.
- Run cargo audit against the final Cargo.lock; retain the database revision, tool version, exit status and sanitized findings. Do not silently suppress advisories.
- Assess Windows target relevance without ignoring build dependencies or target-independent vulnerabilities. Resolve applicable findings and rerun locked tests and release build.
- Review dependency features and lockfile changes; no direct HTTP client, OAuth implementation, credential-store wrapper or automatic updater should be reintroduced.
- Approval remains blocked until final audit and runtime acceptance. No Windows CI is triggered by this inventory.

## Automated checks — 2026-11-27

- `cargo-audit 0.23.0`, installed from crates.io with `--locked`.
- Command: `cargo audit --json`; exit code `0`.
- Advisory database revision: `c9f6c91a44dbad9d71742329e9df061279f9c4be`.
- Database reported 1,568 advisories; the lockfile contained 51 dependency
  entries; vulnerabilities found: `0`.
- `cargo-deny 0.22.1`, installed from crates.io with `--locked`.
- Command: `cargo deny check`; exit code `0` using the repository
  `deny.toml` policy.
- Cargo-deny result: advisories `ok`, bans `ok`, licenses `ok`, sources `ok`.
- Cargo-deny warning: duplicate `syn` versions (`2.0.117` and `3.0.4`),
  retained because they arrive through independent upstream dependency paths.
