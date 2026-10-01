# Actions

Actions let operators perform a small set of pre-approved tasks, such as
restarting nginx, without shell access. They are disabled until you
configure them, and the actions UI is hidden entirely when none exist.

Actions are the **only** feature of helmsight that can change monitored
hosts, so read this whole page before enabling them.

```toml
[[actions]]
id = "restart-nginx"
label = "Restart nginx"
description = "Restarts the web server. Takes about two seconds."
command = "sudo -n /usr/bin/systemctl restart nginx.service"
groups = ["web"]       # required: hosts, groups and/or tags
role = "operator"      # operator or admin
timeout = "60s"
```

- The command is fixed. The UI can only choose an action and a host; it
  cannot pass arguments, and commands have no templates or variables.
- Every action must name its target hosts, groups or tags explicitly; there
  is no "all hosts" default.
- Before running, a dialog shows the exact command and host and asks for
  confirmation.
- The output (up to 64 KiB) is shown in the UI; its first 4 KiB are also
  written to the audit log.
- Privileged commands must start with `sudo -n`, with a sudoers rule for
  exactly that command on each host:

```text
Cmnd_Alias HELMSIGHT_ACTIONS = /usr/bin/systemctl restart nginx.service
monitor ALL=(root) NOPASSWD: HELMSIGHT_ACTIONS
```

Add the rule with `visudo -f /etc/sudoers.d/helmsight`; `visudo` refuses to
save a file with syntax errors. Example file:
[`examples/sudoers`](https://github.com/enderkus/helmsight/blob/main/examples/sudoers).

The `-n` option stops sudo from asking for a password: without a matching
rule, the command fails immediately instead of waiting.

## Designing safe actions

- **Never** allow a shell (`/bin/sh`, `bash`), an editor (`vi`, `nano`), a
  pager (`less`), wildcards (`*`) or `ALL` in sudoers. Each of them means
  full root access.
- Write commands with full paths and fixed arguments, such as
  `systemctl restart nginx.service`. A rule like `systemctl restart *`
  allows restarting every service.
- Turn reversible tasks with limited impact into actions: restarts, cache
  flushes, status queries. Use your existing change management for tasks
  that delete data or change configuration.
- Use `role = "admin"` for risky actions.
- Give the monitoring account sudo rights only on hosts where actions are
  defined. Hosts that are only monitored need no sudoers rule.

An example action that needs no privileges:

```toml
[[actions]]
id = "disk-usage"
label = "Show disk usage"
description = "Shows how full the filesystems are."
command = "df -h"
groups = ["web", "db"]
```

## Audit log

Every run is written to the audit log when it starts and when it finishes,
with the user, host, command, exit status and the first 4 KiB of output.
Administrators read it under **Audit log**. User, host key, silence,
display token and secret changes are recorded too.

The log is append-only: the database rejects updates and deletes, and every
entry carries a SHA-256 hash chained to the previous one. **Verify
integrity** in the UI, or `helmsight audit verify`, detects tampering.

The chain makes it noticeable when someone with direct access to the
database file silently changes or deletes a past entry; it cannot stop
someone with root on the central server from rewriting the whole log. If
your audit requirements are strict, export the log regularly (for example
through the API).
