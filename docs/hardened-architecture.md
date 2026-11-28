# Hardened architecture and current blocker

## Trust boundary

The monitor is intended to render Codex quota data only. Authentication and
provider network access must remain inside the explicitly approved official
Codex executable. The monitor may send exactly three stdio JSON-RPC messages:

1. `initialize`
2. `initialized`
3. `account/rateLimits/read`

It must not read credential files, construct HTTP requests, create threads,
start turns, invoke inference, or expose a generic JSON-RPC method interface.

## Executable identity

The current development integration target is Codex CLI `0.172.0`. The executable
must be represented by a user-approved absolute path, SHA-256 digest, and
version. The identity is configuration data, not committed source data. The
identity is revalidated immediately before each launch; a missing or changed
executable fails closed.

## Startup integration blocker

The official app-server loads Codex startup configuration. Current public
app-server documentation and source show MCP and plugin startup machinery, but
the stable CLI help exposes no supported all-integrations-off switch. An empty
`mcp_servers={}` override is not a reliable way to clear inherited servers;
per-server overrides require knowing every configured server and do not prove
that plugin-injected integrations are absent.

The current official app-server README documents plugin suppression through
thread-scoped configuration (`plugins._default.enabled` and
`disabledPluginIds`), while also documenting startup configuration and MCP
startup status. The monitor does not create a thread or execute a turn, so
thread-scoped suppression does not establish that app-scoped startup work is
absent for an account-only request.

Reference: <https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md>

The current app-server implementation also constructs a processor with a
`plugin_startup_tasks` path during server setup, which is additional evidence
that account-only RPC selection alone is not a startup-isolation guarantee.
Reference: <https://github.com/openai/codex/blob/main/codex-rs/app-server/src/lib.rs>

Therefore live monitor integration is blocked until one of these is established
from the official client:

- a supported invocation that suppresses all MCP/plugin/hook startup while
  preserving the existing authenticated account; or
- a supported isolated configuration mechanism that does not require the
  monitor to read or copy credential material.

The monitor must not silently proceed on the assumption that app-server startup
is stateless or quota-only.

## Process requirements

The client implementation uses direct executable invocation, redirected stdio,
bounded JSON lines, concurrent stderr draining, fixed protocol messages, and
bounded child cleanup. Before production integration it must add native Windows
Job Object kill-on-close handling and prove cleanup on timeout, cancellation,
monitor exit, and child failure.

## Data handling

Only validated primary and secondary Codex windows may become `UsageData`.
Missing windows remain unavailable. Raw protocol responses, account identifiers,
credentials, stdout, and stderr must never be persisted or logged.

## Remediation status

The app-server launch boundary now rejects every attempt until a verified startup policy is implemented. Synthetic protocol success on CLI 0.172.0 is not approval for authenticated polling. The main window settings and startup registry identifiers are isolated; upstream settings and startup migration are removed. Installation scripts are intentionally removed pending acceptance.

The client now waits for initialization before requesting quotas, enforces frame limits before allocation, uses a nonblocking bounded queue, and closes stdin for graceful shutdown. The launcher assigns a kill-on-close Job Object at process creation and rejects uncontained fallback launches. The 0.172.0 target uses the reviewed plugin-disabled, empty-MCP invocation; authenticated production-path acceptance passed, and the compatibility gate remains mandatory for every launch.
