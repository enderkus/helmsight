# helmsight

**Watch every server from one screen. Install nothing on them.**

helmsight is an agentless, self-hosted web dashboard for a fleet of Linux
servers. One binary on a central machine connects to your servers over plain
SSH, collects metrics and inventory with read-only commands, keeps history in
an embedded SQLite database and serves a fast web UI. Nothing is installed,
copied or written on the monitored servers.

![Fleet overview](images/fleet.png)

## What you get

- **Fleet overview** with status, CPU, memory, load, the fullest disk,
  network, uptime and alerts per host; groups, tags, search and sparklines.
- **Host detail** with live and historical charts, a per-core CPU heatmap,
  processes, listening ports, services, containers and system information.
- **History** for up to 90 days with automatic rollups.
- **Drift tracking**: what changed since yesterday, host-to-host and
  baseline comparison of packages, ports, units, kernel and OS.
- **Security overview**: failed SSH logins, pending security updates,
  unexpected ports, reboot flags and TLS certificate expiry.
- **Alerts** with durations and scopes, acknowledgements, silences and
  notifications by webhook, Slack or email.
- **Actions** (opt-in): pre-approved commands with confirmation and an
  append-only audit log.
- **Users and roles**, TOTP, OpenID Connect single sign-on.
- **Wall display** for NOC screens, a JSON API with OpenAPI, server-sent
  events and a Prometheus endpoint.

## Design principles

1. **Agentless.** Nothing is installed or written on monitored hosts.
2. **Read-only by default.** Only the opt-in Actions feature can change a
   host, and only with commands an administrator wrote down.
3. **No arbitrary command execution.** No terminal, no command box.
4. **Secure by default.** Host keys are verified, the server binds to
   loopback, secrets are encrypted at rest.
5. **Remote output is untrusted.** It is size-limited, parsed in Rust and
   never rendered as HTML.
6. **Low overhead.** One persistent SSH session and one short shell script
   per host every few seconds.
7. **Portable.** Debian, Ubuntu, RHEL/Rocky/Alma, Fedora, openSUSE and
   Alpine (BusyBox).
8. **Single binary.** The web UI is embedded; deployment is copying one file.

Continue with [Installation](installation.md) or jump straight to the
[Quick start](quick-start.md).
