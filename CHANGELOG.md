# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The README quick start now changes into the directory unpacked from the
  release archive before running the binary.

## [0.1.1] - 2026-10-01

### Fixed

- Sign-in and every other state-changing request were refused as
  cross-origin when the web UI was served over TLS with HTTP/2.

## [0.1.0] - 2026-10-01

### Added

- Agentless collection over SSH with one persistent session per host and a
  single read-only POSIX shell script per tick, tested on Debian, Ubuntu,
  Rocky, Fedora, openSUSE and Alpine.
- Local mode (`serve --local`) that monitors the machine it runs on.
- Fleet overview with groups, tags, search, sparklines and distinct states
  for unreachable hosts, authentication failures, host key problems and
  partial collections.
- Host detail with live and historical charts, per-core CPU heatmap, top
  processes, listening ports, services, containers and system information.
- History in SQLite with 1-minute and 5-minute rollups and configurable
  retention.
- Inventory snapshots, change timeline and host-to-host or baseline
  comparison.
- Security overview: failed SSH logins, pending and security updates,
  reboot-required flags, ports not on the baseline and TLS certificate
  expiry.
- Alert rules with durations and scopes, built-in rules, acknowledgements,
  silences, reminders, and webhook, Slack and email notifications.
- Opt-in actions with confirmation and a hash-chained, append-only audit log.
- Viewer, operator and admin roles; Argon2id passwords; TOTP; OpenID Connect
  single sign-on with role mapping.
- Wall display mode with revocable, group-scoped display tokens.
- Versioned JSON API with OpenAPI document, server-sent events and a
  Prometheus endpoint.
- TLS with provided certificates or a generated self-signed certificate.
- Secrets encrypted at rest with XChaCha20-Poly1305.
- CLI: `serve`, `init`, `user`, `hosts test`, `config check`, `secret`,
  `audit verify`.

[Unreleased]: https://github.com/enderkus/helmsight/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/enderkus/helmsight/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/enderkus/helmsight/releases/tag/v0.1.0
