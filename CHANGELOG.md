# Changelog

All notable changes to `pacto-bot-api` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-28

### Added

- Initial `pacto-bot-api` daemon: a standalone Rust/Tokio service that multiplexes multiple Pacto bot identities over a shared Nostr backend.
- JSON-RPC 2.0 handler API over newline-delimited frames on two transports:
  - Unix domain socket at `$DATA_DIR/pacto-bot-api.sock` with `0o600` permissions.
  - Optional localhost HTTP server at `127.0.0.1:9800` protected by `X-Pacto-Bot-Secret`.
- `ClientManager` for static multi-bot configuration loaded from `pacto-bot-api.toml`.
- Three signing backends per bot identity:
  - Local test key (`nsec`) for development, with `zeroize` clearing on drop.
  - Local NIP-46 bunker.
  - Remote production NIP-46 bunker with strict `npub` mismatch rejection.
- `HandlerRegistry` and fan-out event dispatch: handlers register for event types and bot identities, and events are dispatched to all matching handlers.
- Per-call capability authorization for mutating operations (`agent.send_dm`, `agent.set_profile`, `agent.error`).
- Per-handler (10 ops/sec, burst 20) and per-bot aggregate rate limiting.
- SQLite persistence in WAL mode for cursors, handler registrations, and config, with restart recovery and `npub` mismatch detection.
- `pacto-bot-admin` CLI for bot lifecycle operations: `new`, `publish-profile`, `test-bunker`, `export`, `import`, `rotate-http-token`, `status`, and `diagnose --format json`.
- Machine-readable contract artifacts under `schemas/` (config, JSON-RPC catalog, metrics, service compatibility).
- Structured runtime metrics via `agent.metrics` and periodic `$DATA_DIR/reports/latest.json` flushes.
- Graceful shutdown on SIGTERM/SIGINT with cursor persistence and `agent.status` notifications.
- Default test suite running in-process against mock relay and mock bunker implementations, plus gated integration tests for `pacto-dev-env`.
- Secret-redaction test suite verifying that `nsec`, bunker URIs, and the HTTP token never leak into logs, error responses, or binary strings.

### Security

- Established Phase 1 trust boundaries: Unix socket uses kernel file permissions; HTTP transport uses a CSPRNG-generated 256-bit hex secret stored with `0o600` permissions.
- Config file permissions enforced (`0o600` or stricter) on daemon startup.
- Daemon-wide exclusive lock on `$DATA_DIR/daemon.lock` to prevent concurrent instances.

[Unreleased]: https://github.com/covenant-gov/pacto-bot-api/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/covenant-gov/pacto-bot-api/releases/tag/v0.1.0
