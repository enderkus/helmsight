# Troubleshooting

Start with `helmsight config check` and `helmsight hosts test`. The first
validates the configuration, the second connects to every host and prints
one line per host with the same states the UI shows.

## A host shows "Down"

helmsight could not open a TCP connection or complete the SSH handshake
within `ssh.connect_timeout`.

- From the helmsight server, run `ssh -p <port> monitor@<address> true`.
- Check firewalls between the two machines and the `address` and `port`
  of the host.
- Unreachable hosts are retried with exponential backoff up to 5 minutes,
  so a recovered host can take a few minutes to come back.

## A host shows "Auth failed"

The SSH server rejected every key helmsight offered.

- Is the public key in `~monitor/.ssh/authorized_keys` on the host, with
  mode 600 and owned by the account?
- Does the `from="…"` option match the address the host sees? Behind NAT
  this is the translated address.
- Is the account locked or its shell `/usr/sbin/nologin`? helmsight needs
  a working POSIX shell (`/bin/sh`). Accounts created on Alpine with
  `adduser -D` are locked; see
  [Monitoring a fleet](fleet-setup.md#2-create-the-monitoring-account-on-each-host).
- The SSH server's log usually names the reason:
  `journalctl -u ssh -u sshd --since "10 min ago"`, `/var/log/auth.log` or
  `/var/log/secure`.
- Can the helmsight process read the key file? With the systemd unit, the
  file must be readable by the `helmsight` group.
- After authentication failures helmsight waits 1 to 5 minutes before
  retrying, so it does not trigger fail2ban or similar tools.

## "Key not trusted" or "Key changed"

- **Key not trusted**: the host is new. Run `helmsight hosts test --trust`
  or approve the key under **Host keys** in the UI, after comparing the
  fingerprint with `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on
  the host.
- **Key changed**: collection stopped on purpose. If the host was
  reinstalled or its keys were rotated, verify the new fingerprint on the
  host and approve it under **Host keys**. If you cannot explain the change,
  treat it as a possible man-in-the-middle attack.

## A view shows "n/a"

"n/a" always comes with the reason reported by the host. Common ones:

| Reason | Fix |
|---|---|
| system journal not readable | Add the account to `systemd-journal` (or `adm` on Debian/Ubuntu) |
| docker: permission denied | Add the account to `docker` only if you accept root-equivalent access |
| no container runtime found | Docker or Podman is not installed; nothing to fix |
| systemctl: Failed to connect to bus | D-Bus is not running on the host (common in containers) |

Listening ports show process owners only for processes of the monitoring
account; that is a kernel restriction for unprivileged users.

## Pending updates are empty on Rocky, Alma, RHEL or Fedora

`dnf` and `yum` write log files even for read-only queries, so helmsight
does not run them unless you set `collect.dnf_updates = true`. Updates are
checked every 6 hours (`collect.updates_interval`).

## I lost the setup link

As long as no user exists, `helmsight serve` prints a new setup link at
every start. Alternatively, create the administrator from the command
line:

```sh
helmsight user add alice --role admin
```

## I forgot a password or lost a TOTP device

```sh
helmsight user reset-password alice --reset-totp
```

This sets a new password, removes TOTP enrolment and ends the user's
sessions.

## Sign-in fails with "Cross-origin request refused"

The browser's `Origin` does not match the address helmsight sees. Behind a
reverse proxy, set `server.public_url` to the exact external URL (for
example `https://monitor.example.com`) and keep the proxy's `Host` header.

## Single sign-on fails

- `server.public_url` must be set, and
  `<public_url>/api/v1/auth/oidc/callback` must be registered as the
  redirect URI at the identity provider.
- A user without a matching value in `role_map` is denied unless
  `default_role` is set. Check the claim named by `role_claim` in the
  identity provider's token.

## Address already in use

If start-up fails because the address is in use, another process (or an
older helmsight still running) holds the port:

```sh
sudo ss -ltnp 'sport = :8080'
```

Stop that process or choose another port with `--listen` or
`server.listen`.

## Getting more detail

Run the server with more logging:

```sh
HELMSIGHT_LOG=debug helmsight serve
```

Logs never contain secrets or collected output. When reporting a problem,
include `helmsight --version`, the distribution of the affected host and the
output of `helmsight hosts test <name>`.
