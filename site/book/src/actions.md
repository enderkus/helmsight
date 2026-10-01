# Actions

Actions let operators perform a small set of pre-approved tasks, such as
restarting nginx, without shell access. They are disabled until you
configure them, and the actions UI is hidden when none exist.

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
  cannot pass arguments.
- Before running, a dialog shows the exact command and host.
- Privileged commands must use `sudo -n`, with a sudoers rule for exactly
  that command on each host:

```text
Cmnd_Alias HELMSIGHT_ACTIONS = /usr/bin/systemctl restart nginx.service
monitor ALL=(root) NOPASSWD: HELMSIGHT_ACTIONS
```

See [`examples/sudoers`](https://github.com/enderkus/helmsight/blob/main/examples/sudoers).

## Audit log

Every run is written to the audit log when it starts and when it finishes,
with the user, host, command, exit status and the first 4 KiB of output.
Administrators read it under **Audit log**. User, host key, silence,
display token and secret changes are recorded too.

The log is append-only: the database rejects updates and deletes, and every
entry carries a SHA-256 hash chained to the previous one. **Verify
integrity** in the UI, or `helmsight audit verify`, detects tampering.
