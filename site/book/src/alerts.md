# Alerts and notifications

## Rules

Rules live in the configuration file:

```toml
[[alerts.rules]]
id = "disk-full"
expr = "disk_used_pct > 90 for 10m"
severity = "critical"
summary = "Filesystem almost full"
tags = ["env:prod"]          # optional scope: hosts, groups, tags
```

`expr` is `<metric> <operator> <threshold> [for <duration>]`. A rule fires
when the condition holds for the whole duration and resolves when it no
longer holds. Per-instance metrics such as `disk_used_pct` fire once per
mount point. The [configuration reference](configuration.md#metrics-available-to-rules)
lists every metric; the UI shows them under **Alerts → Rules**.

### Writing rules

- **Operators**: `>`, `>=`, `<`, `<=`, `==`, `!=`.
- **Threshold units**: `K`, `M`, `G`, `T` (powers of 1000) and `Ki`, `Mi`,
  `Gi`, `Ti` (powers of 1024). For example, `net_rx_bytes > 100M` means
  100 MB per second.
- **Duration** (`for`) avoids alerts on short spikes. Without a duration,
  the rule fires as soon as the condition holds.
- **Scope**: hosts that match any entry in `hosts`, `groups` or `tags` are
  included. Without any of them, the rule applies to every host.
- **Severity**: `info`, `warning` (default) or `critical`.

While a host is unreachable, its metric alerts keep their state instead of
resolving, so a "disk full" alert does not close and reopen during an
outage.

### Example rules

```toml
# CPU above 90% for 15 minutes
[[alerts.rules]]
id = "cpu-high"
expr = "cpu_pct > 90 for 15m"

# Load above 2 per core
[[alerts.rules]]
id = "load-high"
expr = "load1_per_core > 2 for 10m"

# Memory pressure
[[alerts.rules]]
id = "mem-high"
expr = "mem_used_pct > 95 for 5m"
severity = "critical"

# Inode exhaustion (blocks file creation regardless of free space)
[[alerts.rules]]
id = "inodes"
expr = "disk_inodes_used_pct > 90 for 10m"

# CPU steal on virtual machines (overloaded hypervisor)
[[alerts.rules]]
id = "steal"
expr = "cpu_steal_pct > 10 for 15m"

# Production hosts with pending security updates
[[alerts.rules]]
id = "security-updates"
expr = "pending_security_updates > 0"
severity = "info"
tags = ["env:prod"]

# Hosts that need a reboot
[[alerts.rules]]
id = "reboot"
expr = "reboot_required == 1 for 1d"
severity = "info"

# Brute-force SSH attempts
[[alerts.rules]]
id = "ssh-attempts"
expr = "ssh_failed_logins_1h > 50"

# Clock skew (breaks TLS, Kerberos and log ordering)
[[alerts.rules]]
id = "clock-skew"
expr = "clock_skew_secs > 30"

# Stopped containers
[[alerts.rules]]
id = "containers"
expr = "containers_not_running > 0 for 5m"
groups = ["app"]
```

### Built-in rules

Built-in rules need no configuration:

| Rule | Severity | When |
|---|---|---|
| `host_unreachable` | critical | The host has been unreachable for `alerts.unreachable_after` (2 minutes by default), or authentication fails |
| `host_key_changed` | critical | The host key differs from the trusted key |
| `host_key_unknown` | warning | The host key has not been approved yet |
| `failed_units` | warning | One alert per failed systemd unit or crashed OpenRC service |
| `cert_expiry` | warning / critical | A TLS certificate expires within `cert_warning_days` (21) / `cert_critical_days` (7) days; warning when the check fails |

## Working with alerts

- **Acknowledge** (operator) an alert to show that someone is on it;
  reminders stop until it resolves.
- **Silence** (operator) a rule, a host or both for a period, with a
  reason. Silenced alerts are still recorded but not notified. Useful for
  planned maintenance.
- Resolved alerts stay visible for seven days in the UI and for
  `retention.events` in the database.

## Notifications

```toml
[[notify]]
id = "ops"
type = "slack"                 # webhook, slack or email
url = "secret:slack-webhook"   # stored encrypted: helmsight secret set slack-webhook
min_severity = "warning"
```

Notifications go out when an alert fires, when it resolves, and as
reminders every `alerts.repeat_interval` (4 hours by default) while it is
unacknowledged. Deliveries are retried and recorded with each alert. Admins
can send a test notification with `POST /api/v1/notify/<id>/test`.

### Channel examples

**Slack** (or tools that accept Slack-compatible webhooks, such as
Mattermost and Rocket.Chat):

```sh
helmsight secret set slack-webhook     # paste the webhook URL
```

```toml
[[notify]]
id = "slack-ops"
type = "slack"
url = "secret:slack-webhook"
min_severity = "warning"
```

**Email**:

```toml
[[notify]]
id = "mail-oncall"
type = "email"
smtp_host = "smtp.example.com"
smtp_port = 587
smtp_security = "starttls"
smtp_username = "helmsight@example.com"
smtp_password = "secret:smtp"
from = "helmsight@example.com"
to = ["oncall@example.com"]
min_severity = "critical"
```

**Generic webhook** (your own automation, incident management tools):

```toml
[[notify]]
id = "hook"
type = "webhook"
url = "https://hooks.example.com/helmsight"
headers = { Authorization = "secret:hook-token" }
groups = ["db"]                # only alerts of the db group
```

The JSON webhook payload is documented in the
[configuration reference](configuration.md#notify).

When a channel has `hosts`, `groups` or `tags`, only alerts of those hosts
are sent to it. Alerts that do not belong to a host, such as certificate
alerts, only go to unscoped channels.
