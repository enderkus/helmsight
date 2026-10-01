# Configuration reference

helmsight reads one TOML file. The first of these is used:

1. `--config <path>`
2. `$HELMSIGHT_CONFIG`
3. `/etc/helmsight/helmsight.toml`
4. `./helmsight.toml`

*[Türkçe sürüm](https://enderkus.github.io/helmsight/tr/docs/configuration.html)*

[`examples/helmsight.toml`](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.toml) is a commented
example of every option. Validate a file with `helmsight config check`;
errors show the file, line, column and key:

```text
error: helmsight.toml:23:1: `hosts[1].name`: duplicate host name `web-1`
error: helmsight.toml:41:8: `alerts.rules[0].expr`: unknown operator `=>` (use > >= < <= == !=)
```

Unknown keys are errors, so typos never pass silently.

**Durations** are strings such as `"500ms"`, `"30s"`, `"5m"`, `"1h30m"`,
`"7d"`, `"2w"`.

**Secrets** may be literal strings, but references are preferred:

| Form | Meaning |
|---|---|
| `"secret:<name>"` | Stored encrypted in the database with `helmsight secret set <name>` |
| `"env:<VAR>"` | Value of an environment variable |
| `"file:<path>"` | Contents of a file (trailing newline removed) |

Changes to the configuration take effect after a restart.

## `[server]`

| Key | Default | Description |
|---|---|---|
| `listen` | `"127.0.0.1:8080"` | Address and port to listen on |
| `data_dir` | `"data"` | Database, key file, `known_hosts` and TLS files; relative to the config file |
| `public_url` | none | External `https://` URL; required for OIDC, used in notification links and origin checks |
| `trusted_proxies` | `[]` | IP addresses whose `X-Forwarded-For` header is trusted |

### `[server.tls]`

| Key | Default | Description |
|---|---|---|
| `mode` | `"auto"` | `auto` (HTTP on loopback, self-signed otherwise), `self-signed`, `files` or `off` |
| `cert`, `key` | none | PEM files for `mode = "files"` |
| `allow_insecure_http` | `false` | Permit `mode = "off"` on a non-loopback address (behind a TLS proxy) |

## `[auth]`

| Key | Default | Description |
|---|---|---|
| `session_ttl` | `"12h"` | Maximum session lifetime (min 5m) |
| `idle_timeout` | `"2h"` | Sessions end after this much inactivity (min 1m) |
| `require_totp_for_admins` | `false` | Admins with local passwords must enrol TOTP before using the UI |
| `disable_local_login` | `false` | Only allow OIDC sign-in |

### `[auth.oidc]`

| Key | Default | Description |
|---|---|---|
| `issuer` | required | Issuer URL; discovery uses `<issuer>/.well-known/openid-configuration` |
| `client_id` | required | |
| `client_secret` | none | Secret reference recommended |
| `scopes` | `["openid", "profile", "email"]` | Must include `openid` |
| `role_claim` | `"groups"` | Claim with group or role names; dotted paths such as `realm_access.roles` work |
| `role_map` | `{}` | Claim value → `viewer`, `operator` or `admin`; the highest match wins |
| `default_role` | none | Role for users without a match; unset denies access |
| `label` | `"Single sign-on"` | Text of the sign-in button |

Register `<public_url>/api/v1/auth/oidc/callback` as the redirect URI.
Roles are updated from the claim at every sign-in.

## `[ssh]`

| Key | Default | Description |
|---|---|---|
| `user` | `"monitor"` | Remote user (overridable per host) |
| `identity_files` | `[]` | Private key files, tried in order |
| `use_agent` | `true` | Also try keys from `SSH_AUTH_SOCK` |
| `known_hosts_files` | `[]` | Extra OpenSSH known_hosts files, read-only |
| `accept_new_host_keys` | `false` | Trust keys of new hosts automatically; changed keys are still refused |
| `connect_timeout` | `"10s"` | TCP connect, key exchange and authentication |
| `command_timeout` | `"30s"` | One collection (updates checks get at least 5 minutes) |
| `keepalive` | `"30s"` | SSH keepalive interval |

## `[collect]`

| Key | Default | Description |
|---|---|---|
| `interval` | `"5s"` | Metrics: CPU, memory, load, disks, network, TCP, processes |
| `medium_interval` | `"60s"` | Listening ports, services, containers, sessions |
| `inventory_interval` | `"15m"` | OS, kernel, packages, enabled units, reboot flags |
| `auth_interval` | `"5m"` | Failed SSH logins |
| `updates_interval` | `"6h"` | Pending updates |
| `max_parallel` | `64` | Hosts collected at the same time |
| `dnf_updates` | `false` | Run `dnf`/`yum` for pending updates (they write log files on the host) |

Failed hosts are retried with backoff: exponential up to 5 minutes when
unreachable, 1 to 5 minutes after authentication failures (to avoid
triggering intrusion prevention) and every minute for host key problems.

## `[retention]`

| Key | Default | Description |
|---|---|---|
| `raw` | `"24h"` | Every sample (min 1h) |
| `minute` | `"7d"` | 1-minute average, minimum and maximum |
| `five_minute` | `"90d"` | 5-minute average, minimum and maximum |
| `events` | `"90d"` | Inventory changes, resolved alerts, failed logins, notifications |

Charts pick the finest resolution that covers the requested range.

## Hosts

### `[[hosts]]`

| Key | Default | Description |
|---|---|---|
| `name` | required | Unique; letters, digits, `.`, `-`, `_` |
| `address` | the name | Hostname or IP address |
| `port` | `22` | |
| `user` | `ssh.user` | |
| `identity_file` | none | Key tried before the shared keys |
| `groups` | `[]` | Used for filtering, rules, actions, channels and display tokens |
| `tags` | `[]` | Free-form, typically `key:value` such as `env:prod` |
| `baseline` | none | Host used as the reference for drift and new-port detection |
| `disabled` | `false` | Keep the host listed but do not collect |

### `hosts_file`

A top-level `hosts_file = "hosts.toml"` loads additional `[[hosts]]` from
another file (relative to the configuration file).

### `[ssh_config_import]`

| Key | Default | Description |
|---|---|---|
| `path` | `"~/.ssh/config"` | OpenSSH client configuration |
| `hosts` | required | Glob patterns of aliases to import, e.g. `["web-*"]` |
| `groups`, `tags` | `[]` | Added to every imported host |

`HostName`, `User`, `Port` and `IdentityFile` are honoured, with OpenSSH
"first value wins" semantics across matching `Host` blocks. Explicit
`[[hosts]]` with the same name take precedence. Hosts with `ProxyJump` are
skipped with a warning; `Match` and `Include` are not evaluated.

## Alerts

### `[alerts]`

| Key | Default | Description |
|---|---|---|
| `unreachable_after` | `"2m"` | Delay before `host_unreachable` fires |
| `failed_units` | `true` | Alert on failed systemd units and crashed OpenRC services |
| `cert_warning_days` | `21` | |
| `cert_critical_days` | `7` | |
| `repeat_interval` | `"4h"` | Remind about unacknowledged firing alerts; `"0s"` disables |

Built-in rules: `host_unreachable` (critical; also for authentication
failures), `host_key_changed` (critical), `host_key_unknown` (warning),
`failed_units` (warning, one alert per unit), `cert_expiry` (warning or
critical by days left; warning when the check fails).

### `[[alerts.rules]]`

| Key | Default | Description |
|---|---|---|
| `id` | required | Unique |
| `expr` | required | `<metric> <op> <threshold> [for <duration>]` |
| `severity` | `"warning"` | `info`, `warning` or `critical` |
| `summary` | none | Prefix for the generated description |
| `hosts`, `groups`, `tags` | all hosts | Scope; a host matches if it matches any entry |

Operators: `>`, `>=`, `<`, `<=`, `==`, `!=`. Thresholds accept the
suffixes `K`, `M`, `G`, `T` (powers of 1000) and `Ki`, `Mi`, `Gi`, `Ti`
(powers of 1024). Per-instance metrics produce one alert per instance (for
example per mount point). While a host is unreachable, its metric alerts
keep their state instead of resolving.

### Metrics available to rules

| Metric | Unit | Per | Description |
|---|---|---|---|
| `cpu_pct` | % | host | CPU busy time, all cores |
| `cpu_iowait_pct` | % | host | CPU time waiting for I/O |
| `cpu_steal_pct` | % | host | CPU time stolen by the hypervisor |
| `mem_used_pct` | % | host | Memory in use, excluding reclaimable cache |
| `swap_used_pct` | % | host | Swap in use |
| `load1`, `load5`, `load15` | | host | Load averages |
| `load1_per_core` | | host | 1-minute load divided by cores |
| `disk_used_pct` | % | mount | Filesystem space used |
| `disk_inodes_used_pct` | % | mount | Filesystem inodes used |
| `disk_util_pct` | % | device | Block device utilisation |
| `net_rx_bytes` | B/s | interface | Bytes received per second |
| `net_tx_bytes` | B/s | interface | Bytes sent per second |
| `net_errors` | 1/s | interface | Receive and transmit errors per second |
| `tcp_established` | | host | Established TCP connections |
| `process_count` | | host | Processes |
| `failed_units` | | host | Failed systemd units or crashed OpenRC services |
| `pending_updates` | | host | Pending package updates |
| `pending_security_updates` | | host | Pending security updates |
| `reboot_required` | | host | 1 when a reboot is required |
| `ssh_failed_logins_1h` | | host | Failed SSH logins in the last hour |
| `uptime_secs` | s | host | Seconds since boot |
| `clock_skew_secs` | s | host | Difference between host and server clocks |
| `containers_not_running` | | host | Containers that exist but are not running |

## `[[notify]]`

| Key | Applies to | Description |
|---|---|---|
| `id` | all | Unique |
| `type` | all | `webhook`, `slack` or `email` |
| `min_severity` | all | Default `warning` |
| `hosts`, `groups`, `tags` | all | Only alerts of these hosts (host-less alerts such as certificates go to unscoped channels) |
| `url` | webhook, slack | `https://` URL (plain `http` only for localhost) |
| `headers` | webhook | Extra HTTP headers; values may be secret references |
| `smtp_host`, `smtp_port` | email | |
| `smtp_security` | email | `starttls` (default), `tls`, or `none` for a relay on localhost |
| `smtp_username`, `smtp_password` | email | |
| `from`, `to` | email | Sender and recipients |

Notifications are sent when an alert fires, when it resolves (if the firing
notification was sent) and as reminders. Silenced alerts are recorded but
not notified. Failed deliveries are retried three times and listed with the
alert. Send a test from the API: `POST /api/v1/notify/<id>/test`.

Webhook payload:

```json
{
  "version": 1,
  "source": "helmsight",
  "status": "firing",
  "alert": {
    "id": 42,
    "rule": "disk-full",
    "host": "web-1",
    "instance": "/var",
    "severity": "critical",
    "summary": "disk_used_pct on /var is 93.1% (> 90.0%)",
    "value": 93.1,
    "started_at": "2026-09-30T16:56:12Z",
    "resolved_at": null,
    "acknowledged_by": null
  },
  "url": "https://monitor.example.com/alerts?id=42"
}
```

`status` is `firing`, `resolved`, `reminder` or `test`.

## `[[certs]]`

| Key | Default | Description |
|---|---|---|
| `endpoint` | required | `host:port` |
| `server_name` | endpoint host | SNI name and name to validate |
| `interval` | `"6h"` | |

The certificate is read even when it does not validate; trust problems are
shown separately from connection failures.

## `[[actions]]`

| Key | Default | Description |
|---|---|---|
| `id` | required | Unique |
| `label` | required | Button text |
| `description` | `""` | Shown in the UI |
| `command` | required | Exact single-line command; privileged commands must start with `sudo -n ` |
| `hosts`, `groups`, `tags` | required | Hosts where the action is offered; at least one entry |
| `role` | `"operator"` | `operator` or `admin` |
| `timeout` | `"60s"` | 1s to 1h |

## `[prometheus]`

| Key | Default | Description |
|---|---|---|
| `enabled` | `false` | Serve `/metrics` |
| `token` | required when enabled | Bearer token |

```yaml
scrape_configs:
  - job_name: helmsight
    scheme: https
    authorization:
      credentials_file: /etc/prometheus/helmsight.token
    static_configs:
      - targets: ["monitor.example.com"]
```

Exposed metrics are prefixed with `helmsight_`: `host_up`,
`host_alerts_firing`, `cpu_percent{mode}`, `memory_*_bytes`, `load1/5/15`,
`filesystem_{size,used,avail}_bytes{mount}`,
`disk_{read,written}_bytes_per_second{device}`,
`network_{receive,transmit}_bytes_per_second{interface}`,
`tcp_connections{state}`, `pending_updates`, `pending_security_updates`,
`reboot_required` and more, all labelled with `host`.

## Command line

| Command | Description |
|---|---|
| `serve [--local] [--listen ADDR]` | Run the server. `--local` monitors only this machine by reading `/proc` (no SSH, no configuration needed). |
| `init [--force]` | Interactively create a configuration file, the data directory, the key file and the first administrator |
| `user add <name> [--role viewer\|operator\|admin] [--password-stdin]` | Create a local user |
| `user remove <name>` | Delete a user (the last admin cannot be removed) |
| `user reset-password <name> [--password-stdin] [--reset-totp]` | Set a new password and end the user's sessions |
| `user list` | List users |
| `hosts test [--trust] [names...]` | Connect to hosts and report status; `--trust` confirms unknown host keys interactively |
| `config check` | Validate the configuration and check that referenced secrets exist |
| `secret set <name> [--stdin]` / `secret list` / `secret delete <name>` | Manage encrypted secrets |
| `audit verify` | Verify the audit log hash chain |

Global options: `--config <path>`, `--data-dir <dir>` (overrides
`server.data_dir`; without a configuration file, commands use the
local-mode default `~/.local/share/helmsight`, or `/var/lib/helmsight` for
root), `--log-format text|json`.

When no user exists, `serve` prints a one-time setup link
(`/setup#token=...`) for creating the first administrator in the browser.

## Environment variables

| Variable | Description |
|---|---|
| `HELMSIGHT_CONFIG` | Configuration file path |
| `HELMSIGHT_DATA_DIR` | Same as `--data-dir` |
| `HELMSIGHT_LOG` | Log filter, e.g. `info`, `debug`, `info,server=debug` |
| `SSH_AUTH_SOCK` | ssh-agent socket used when `ssh.use_agent = true` |

## HTTP API

The web UI uses the same versioned JSON API under `/api/v1`. The OpenAPI
document is served at `/api/v1/openapi.json`. Authenticate with the session
cookie from `POST /api/v1/auth/login`; state-changing requests need the
`X-CSRF-Token` header returned by `GET /api/v1/auth/me`. Live updates are
available as server-sent events at `/api/v1/events`. Wall displays call
`/api/v1/display/state` with `Authorization: Bearer <display token>`.
`/healthz` returns `ok` without authentication.
