# helmsight

**Watch every server from one screen. Install nothing on them.**

helmsight is an agentless, self-hosted web dashboard for a fleet of Linux
servers. A single binary on one central machine connects to your servers over
plain SSH, collects metrics and inventory with read-only commands, keeps
history in an embedded database and serves a fast web UI. Nothing is
installed, copied or written on the monitored servers.

> [!NOTE]
> **Status: early development (0.x).** helmsight is new. It is tested on Debian, Ubuntu, Rocky, Fedora, openSUSE
> and Alpine, but it has had little production use and no independent security
> audit. Configuration options, the API and the database format may change
> between 0.x releases. Try it on non-critical hosts first, keep a backup of the
> data directory, and review the [security model](docs/security.md) and the [collection script](crates/collect/src/remote.sh) before deploying widely.
> Collection is read-only; the opt-in Actions feature runs the commands you
> configure, so enable it with care. helmsight is provided as is, without
> warranty, under the [MIT license](LICENSE).

**Website and documentation: https://enderkus.github.io/helmsight/**

![Fleet overview](docs/screenshots/fleet.png)

| | |
|---|---|
| ![Host detail](docs/screenshots/host.png) | ![Security overview](docs/screenshots/security.png) |
| ![Compare two hosts](docs/screenshots/compare.png) | ![Wall display](docs/screenshots/display.png) |

## What it does

- **Fleet overview**: status, CPU, memory, load, fullest disk, network,
  uptime and alerts for every host, with groups, tags, search and
  sparklines. "Host down", "SSH authentication failed", "host key changed"
  and "collection partially failed" are distinct states.
- **Host detail**: live and historical charts (CPU including iowait and
  steal, per-core heatmap, memory and swap, load, disk usage and I/O,
  network, TCP), top processes, listening ports with owners, systemd or
  OpenRC services, Docker and Podman containers, and system information.
- **History**: raw samples for 24 hours, 1-minute rollups for 7 days and
  5-minute rollups for 90 days by default, in SQLite. Time ranges from 15
  minutes to 30 days with a synchronized crosshair.
- **Drift and change tracking**: packages, kernel, listening ports, enabled
  units and OS release are snapshotted every 15 minutes. Compare two hosts,
  or a host with its baseline, and see what changed since yesterday.
- **Security overview**: failed SSH logins over time, pending (security)
  updates, ports not present on the baseline, reboot-required flags and TLS
  certificate expiry.
- **Alerts**: rules such as `disk_used_pct > 90 for 10m` scoped to hosts,
  groups or tags; built-in rules for unreachable hosts, host key problems,
  failed units and certificate expiry; acknowledgements, silences and
  notifications by webhook, Slack-compatible webhook or email.
- **Actions** (opt-in): let operators run a few pre-approved commands, such
  as restarting nginx, with a confirmation dialog and an append-only audit
  log. Hidden entirely when not configured.
- **Users**: viewer, operator and admin roles; local accounts with Argon2id
  and optional TOTP; OpenID Connect single sign-on with role mapping.
- **Wall display**: a read-only status wall for TV screens, available with a
  revocable token scoped to groups.
- **Integrations**: a versioned JSON API with an OpenAPI document,
  server-sent events and a Prometheus `/metrics` endpoint.

## How it stays agentless and safe

- **One read-only POSIX shell script per tick.** Every collection is a single
  SSH exec of `sh -s` on a persistent session. The script is sent on standard
  input, so it never appears in the remote process list. It reads `/proc`,
  `df`, `ps`, `ss`, `systemctl`, the package database and similar read-only
  sources. It never writes files and degrades to "n/a" with a reason when a
  tool is missing or not permitted. It runs on Debian, Ubuntu,
  RHEL/Rocky/Alma, Fedora, openSUSE and Alpine (BusyBox).
- **The browser only runs code served by helmsight.** Remote hosts return
  plain text, which is parsed and size-limited in Rust. Remote output is
  never rendered as HTML, and a strict Content-Security-Policy forbids
  inline scripts and styles.
- **No command execution from the UI.** There is no terminal, file manager
  or command box. Actions are fixed commands written by an administrator in
  the configuration file; the UI can only pick an action and a host.
- **Host keys are verified.** OpenSSH `known_hosts` semantics apply: unknown
  keys are refused until an administrator confirms the fingerprint (or you
  opt into accept-new), and changed keys are always refused.
- **Secure defaults.** helmsight listens on `127.0.0.1` and uses TLS
  automatically on other addresses. Cookies are HttpOnly and SameSite=Strict,
  every state-changing request needs a CSRF token, sign-in is rate limited,
  and secrets are encrypted at rest with XChaCha20-Poly1305.

Read [docs/security.md](docs/security.md) for the threat model, the exact
commands run on hosts and how to create a restricted monitoring account.

## Quick start

Try it on one Linux machine, without SSH:

```sh
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/helmsight-x86_64-unknown-linux-musl.tar.gz
tar xzf helmsight-x86_64-unknown-linux-musl.tar.gz
cd helmsight-x86_64-unknown-linux-musl
./helmsight serve --local
```

Open the setup link printed in the terminal to create the first
administrator. On ARM servers, replace `x86_64` with `aarch64`.

Monitor a fleet:

```sh
./helmsight init                      # config file, key file, first admin
$EDITOR helmsight.toml                # add [[hosts]] entries
./helmsight hosts test --trust        # check SSH and confirm host keys
./helmsight serve
```

On each monitored host, create an unprivileged account with your public
key (see [Creating the monitoring account](docs/security.md#creating-the-monitoring-account)).
For a long-running installation, use the hardened systemd unit in
[examples/helmsight.service](examples/helmsight.service), or the container
image:

```sh
docker run -d -p 8443:8080 -v helmsight:/var/lib/helmsight \
  -v /etc/helmsight:/etc/helmsight:ro ghcr.io/enderkus/helmsight
```

In the container, set `listen = "0.0.0.0:8080"` and
`data_dir = "/var/lib/helmsight"`; TLS is then enabled automatically
(self-signed unless you configure certificates). The image contains only
the static binary, so `--local` mode is not available in it.

## Configuration

helmsight reads a single TOML file (`--config`, `$HELMSIGHT_CONFIG`,
`/etc/helmsight/helmsight.toml` or `./helmsight.toml`). Hosts can live in
the same file, in a separate inventory file, or be imported from
`~/.ssh/config`. A minimal configuration:

```toml
[server]
listen = "127.0.0.1:8080"
data_dir = "/var/lib/helmsight"

[ssh]
user = "monitor"
identity_files = ["/etc/helmsight/id_ed25519"]

[[hosts]]
name = "web-1"
address = "10.0.0.11"
groups = ["web"]
tags = ["env:prod"]

[[alerts.rules]]
id = "disk-full"
expr = "disk_used_pct > 90 for 10m"
severity = "critical"
```

`helmsight config check` validates the file and points at the exact line
and key of every problem:

```text
error: /etc/helmsight/helmsight.toml:17:1: `alerts.rules[0].expr`: unknown metric `disk_used`; available: cpu_pct, ...
```

- [examples/helmsight.toml](examples/helmsight.toml) documents every option.
- [docs/configuration.md](docs/configuration.md) is the full reference:
  sections, metrics for alert rules, CLI commands, API and environment
  variables.

### Command line

| Command | Purpose |
|---|---|
| `helmsight serve [--local] [--listen ADDR] [--data-dir DIR]` | Run the server and collectors |
| `helmsight init` | Create a configuration, key file and the first administrator |
| `helmsight user add\|remove\|reset-password\|list` | Manage local accounts |
| `helmsight hosts test [--trust] [NAME...]` | Check connectivity, authentication and host keys |
| `helmsight config check` | Validate the configuration and stored secrets |
| `helmsight secret set\|list\|delete` | Manage encrypted secrets referenced as `secret:<name>` |
| `helmsight audit verify` | Verify the audit log hash chain |

## FAQ

**Why not an agent?** Agents must be installed, updated, configured and
trusted on every host, and they widen the attack surface. SSH is already
there, already hardened and already audited. helmsight adds one read-only
login every few seconds.

**How much load does it put on a host?** One short shell script every 5
seconds that reads files under `/proc` and runs a few lightweight tools: a
few milliseconds of CPU. Expensive checks such as pending updates run every
6 hours.

**Does it need root?** No. A dedicated unprivileged account is recommended.
A few details need extra permissions: failed logins need read access to the
journal (`systemd-journal` or `adm` group), process owners of listening
sockets are only visible for the account's own processes, and Docker
containers need access to the Docker socket, which is equivalent to root.
helmsight shows "n/a" with the reason for anything it cannot read.

**Does it really write nothing on the hosts?** helmsight's commands write
nothing. This is verified by an integration test on six distributions. The
host's own login bookkeeping (wtmp, lastlog, the journal, and Ubuntu's
`pam_motd` marker in the account's home directory) records SSH logins as
usual. `dnf` and `yum` always write log files, so pending updates on
RHEL-family hosts are only listed when you set `collect.dnf_updates = true`.

**Can I use a jump host?** Not yet. Hosts with `ProxyJump` in an imported
SSH config are skipped with a warning.

**How many hosts can it handle?** Collection is concurrent, with one
persistent session per host and a configurable limit. Storage uses one
compact row per host per tick. A few hundred hosts on a small VM are
expected to be fine.

**Is there a dark mode?** Yes. The UI follows the OS preference, and you
can override it in the top bar.

## Building from source

Requirements: Rust (stable), Node.js 22 and npm.

```sh
(cd web && npm ci && npm run build)
cargo build --release
./target/release/helmsight --help
```

The web UI is embedded into the binary at compile time. See
[CONTRIBUTING.md](CONTRIBUTING.md) for development, tests and the project
layout.

## License

MIT. See [LICENSE](LICENSE).
