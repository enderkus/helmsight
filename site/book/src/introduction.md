# helmsight

**Watch every server from one screen. Install nothing on them.**

helmsight is an agentless, self-hosted web dashboard for a fleet of Linux
servers. One binary on a central machine connects to your servers over plain
SSH, collects metrics and inventory with read-only commands, keeps history in
an embedded SQLite database and serves a fast web UI. Nothing is installed,
copied or written on the monitored servers.

> **Status: early development (0.x).** helmsight is new. It is tested on Debian, Ubuntu, Rocky, Fedora, openSUSE
> and Alpine, but it has had little production use and no independent security
> audit. Configuration options, the API and the database format may change
> between 0.x releases. Try it on non-critical hosts first, keep a backup of the
> data directory, and review the [security model](security.md) and the [collection script](https://github.com/enderkus/helmsight/blob/main/crates/collect/src/remote.sh) before deploying widely.
> Collection is read-only; the opt-in Actions feature runs the commands you
> configure, so enable it with care. helmsight is provided as is, without
> warranty, under the [MIT license](https://github.com/enderkus/helmsight/blob/main/LICENSE).
> You use it at your own risk.

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

## How it works

1. helmsight keeps **one persistent SSH session** per host. Every few
   seconds it runs `sh -s` on that session and sends a short POSIX shell
   script on standard input. The script is never written to a file on the
   remote host and does not appear in its process list.
2. The script only reads: `/proc` files, `df`, `ps`, `ss`, `systemctl`, the
   package database and similar sources. When a tool is missing or not
   permitted, it reports "n/a" with the reason instead of failing.
3. The output is size-limited, parsed in Rust on the central machine and
   written to SQLite. Older data is condensed into 1-minute and 5-minute
   rollups.
4. The web UI is embedded in the binary. The browser only runs code served
   by helmsight; text coming from remote hosts is never rendered as HTML.

Heavier checks (pending updates, the package inventory) run less often.
The [security model](security.md#what-helmsight-runs-on-monitored-hosts)
lists every command and how often it runs.

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

## About this documentation

The documentation is available in English and
[Turkish](https://enderkus.github.io/helmsight/tr/docs/); the link in the
top bar of every page opens the same page in the other language. The UI
itself is in English. Menu and button names are written in bold as they
appear in the UI, for example **Host keys**. Configuration keys, commands
and API paths are written as code.

Continue with [Installation](installation.md) or jump straight to the
[Quick start](quick-start.md).
