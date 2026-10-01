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

Built-in rules cover unreachable hosts and SSH authentication failures,
changed and untrusted host keys, failed systemd units or crashed OpenRC
services, and TLS certificate expiry.

## Working with alerts

- **Acknowledge** (operator) an alert to show that someone is on it;
  reminders stop until it resolves.
- **Silence** (operator) a rule, a host or both for a period, with a
  reason. Silenced alerts are still recorded but not notified.
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
reminders every `alerts.repeat_interval` while it is unacknowledged.
Deliveries are retried and recorded with each alert. Admins can send a test
notification with `POST /api/v1/notify/<id>/test`.

The JSON webhook payload is documented in the
[configuration reference](configuration.md#notify).
