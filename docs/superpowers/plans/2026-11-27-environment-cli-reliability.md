# Reliable environment CLI workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make environment polling and listing reliable for scripts, and provide a clear path for validating published installs.

**Architecture:** Keep deadline enforcement local to `environments wait` by wrapping each complete existing GET future with the absolute wait deadline. Add list-specific pagination arguments and pass them as query parameters to the existing API endpoint.

**Tech Stack:** Rust, Tokio, reqwest, clap, Axum tests, shell acceptance scripts.

**Spec:** `docs/superpowers/specs/2026-11-27-environment-cli-reliability-design.md`

## Global Constraints

- Preserve the API response shape and existing default behavior.
- `ready` returns success; `failed` prints its response and exits non-zero.
- `--wait-timeout-ms` bounds sleeps and HTTP response waits.
- The public signed-release journey cannot be claimed validated without running on Linux, macOS, and Windows with a test tenant.

## Review Focus

- A failed environment remains available as structured output while the command exits non-zero — test both JSON output and result status.
- An HTTP response that outlasts the deadline does not extend the command — test a deliberately slow local endpoint.
- Page and page size reach the API, while omitted values preserve defaults — test the captured request URI and CLI parser.
- A release acceptance run cannot silently treat only signature verification as the full tenant lifecycle — report unavailable credentials/platforms as an explicit validation gap.

---

### Task 1: Make environment wait deadline-aware and failure-sensitive

**Files:**
- Modify: `src/main.rs` — enforce deadline across polling and classify terminal environment states.
- Test: unit tests in `src/main.rs` and process tests in `tests/environment_cli.rs`.

**Interfaces:**
- Produces: `wait_environment(client: &AuthClient, api_key: &str, id: &str, wait_timeout_ms: u64) -> Result<serde_json::Value, EnvironmentWaitFailure>`.
- `environments wait` retains the last successful JSON value for deadline reporting.

- [x] Add tests for ready, failed process JSON/exit status, deadline exhaustion, slow response, and a stalled response body.
- [x] Prove the failed-status and slow-deadline tests fail when their regressions are reintroduced.
- [x] Wrap the existing complete GET operation in `tokio::time::timeout_at(deadline, ...)`, preserving the client's configured timeout.
- [x] Run targeted tests and the full Rust test suite (70 unit tests plus 3 process tests).

Ruling: Use an outer absolute deadline around the existing GET instead of a request-specific timeout — the existing client has its own configured timeout, and the request future includes response-body decoding — cost if wrong: a timeout configuration interaction could end the wait earlier than expected, which is the requested deadline behavior.

### Task 2: Add environment list pagination and document acceptance limits

**Files:**
- Modify: `src/main.rs` — add `EnvironmentListArgs` and query construction.
- Create: `tests/environment_cli.rs` — exercise process exit/output and capture actual API request URIs.
- Modify: `docs/cli.md` — document flags and defaults.
- Modify: `docs/release.md` — specify native post-release tenant lifecycle acceptance.
- Modify: `scripts/cli-public-acceptance*.sh` only if the existing acceptance harness can exercise the tenant lifecycle without broadening its credential exposure; otherwise record the required environment and limitation in release docs.
- Test: CLI parser and local Axum request capture in `src/main.rs`.

**Interfaces:**
- `mpaka environments list [--page N] [--page-size N] [--json]`.
- Defaults: `page=1`, `page_size=25`, matching the API's existing execution-list convention.

- [x] Add process output/exit tests and HTTP request-capture tests for explicit values and defaults; verify both fail when the behavior is removed.
- [x] Implement query serialization, preserving the response payload.
- [x] Update CLI documentation and add the native lifecycle acceptance procedure to the release runbook.
- [x] Run all Rust tests plus docs, installer, release-workflow, release-fixture, and public-topology fixture checks.
- [x] Report actual public Linux/macOS/Windows lifecycle validation separately; do not infer it from local fixture tests.

Validation gap: the active v0.4.0 signed release and all six artifact hashes were verified, and a clean Linux install reported `mpaka 0.4.0`. The full public acceptance script currently stops because `/latest` returns 404. The authenticated tenant lifecycle could not be run because test credentials are unset; native macOS and Windows were unavailable on this Linux host.

Ruling: Preserve the existing zero exit status for a `cancelled` environment — the requested failure case is `failed`, and changing cancellation policy was not specified — cost if wrong: automation may treat a cancelled environment as successful.
