# Security

This document describes what helmsight does on monitored hosts, what it
cannot do, how to run it with least privilege, and how to report a
vulnerability.

*[Türkçe sürüm](https://enderkus.github.io/helmsight/tr/docs/security.html)*

## Threat model

helmsight has three parties:

1. **The central server** runs the helmsight binary. It holds SSH private
   keys, the database (metrics, inventory, users, sessions, audit log) and a
   key file that encrypts stored secrets.
2. **Monitored hosts** are reached over SSH. Their output is untrusted: a
   compromised host, or an unprivileged local user who controls a process
   name, a log line or a file name, may try to feed hostile data back.
3. **Users** reach the web UI over HTTP(S) with a role: viewer, operator or
   admin. Wall displays use revocable read-only tokens.

helmsight is designed so that:

- **A compromised monitored host cannot attack the browser or the server.**
  Output is size-limited (16 MiB per execution) and parsed by Rust code
  that never panics on malformed input. This is checked by fuzz-style tests
  with truncated and corrupted real output. Values are rendered as text,
  never as HTML. The browser only executes code served by the binary, and
  the Content-Security-Policy forbids inline scripts, inline styles,
  framing and third-party origins. Section markers in the script output
  include a random per-run nonce that is never visible in the remote process
  list, so a process or log line cannot forge a section.
- **A compromised monitored host cannot reach other hosts.** Each host has
  its own SSH session; nothing is forwarded (no agent forwarding, no port
  forwarding).
- **A UI user cannot run arbitrary commands.** There is no terminal, file
  manager or command input. Actions are fixed command strings from the
  configuration file; the API accepts only an action id and a host name,
  and checks that the user's role and the host are permitted.
- **A network attacker cannot impersonate a host.** SSH host keys are
  verified with OpenSSH `known_hosts` semantics. Unknown keys are refused
  until an admin confirms the fingerprint (or `accept_new_host_keys` is
  enabled). A changed key always stops collection, raises a critical alert
  and requires an admin to verify the new fingerprint.

Out of scope: an attacker with root on the central server can read the SSH
keys and the database. Protect that machine accordingly.

### What this means in practice

- If helmsight's monitoring key is stolen, the attacker can only log in as
  the unprivileged `monitor` account and (with `from=`) only from the
  helmsight server's address. Do not skip the `restrict,from=` options.
- If a monitored host is compromised, the worst outcome is false metrics
  for that host; the attacker cannot use it to move on to helmsight or to
  other hosts.
- The central server has read access to the whole fleet; protect it as
  carefully as a bastion host: keep it updated, open only the ports it
  needs and restrict access to the UI.

## What helmsight runs on monitored hosts

Every tick, helmsight opens a channel on the host's persistent SSH session
and runs `sh -s`, sending the collection script on standard input. The
script is in
[`crates/collect/src/remote.sh`](https://github.com/enderkus/helmsight/blob/main/crates/collect/src/remote.sh); review it
before deploying. It contains only reads:

| Group | Interval | Reads |
|---|---|---|
| metrics | 5 s | `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/uptime`, `/proc/diskstats`, `/proc/net/dev`, `/proc/mounts`, `/proc/net/tcp{,6}`, `/proc/[pid]/stat`, `df -Pk`, `df -Pi`, `ps -o pid,user,args`, `date` |
| medium | 60 s | `ss -ltunp` (or `netstat`, or `/proc/net/*`), `systemctl list-units` or `rc-status`, `docker`/`podman ps` and `stats --no-stream`, `who` |
| inventory | 15 min | `/etc/os-release`, `uname`, `/proc/cpuinfo`, `lscpu`, `systemd-detect-virt`, DMI vendor and product, reboot flags, `rpm -q --last kernel-core`, `dpkg-query -W`, `rpm -qa` or `apk info -v`, `systemctl list-unit-files` or `rc-update show` |
| auth | 5 min | `journalctl _COMM=sshd` since the last check, or the tail of `/var/log/auth.log`, `/var/log/secure` or `/var/log/messages` |
| updates | 6 h | `apt-get -s dist-upgrade` (simulation, no locking), `zypper --no-refresh list-updates`, `apk version`, and only if enabled, `dnf -C check-update` |

Properties of the script:

- POSIX `sh` only; it runs under dash, bash and BusyBox ash.
- Every output redirection targets `/dev/null` (enforced by a unit test).
- Missing or forbidden tools produce "n/a" with a reason, never an error.
- Long-running commands are bounded with `timeout` where available.
- The script is never saved on the remote host; it is read from standard
  input and appears in the process list only as `sh -s`.

### What gets written on the host

helmsight's commands write nothing. The integration test suite logs in as an
unprivileged user on Debian, Ubuntu, Rocky, Fedora, openSUSE and Alpine,
runs every collection group and asserts that no file outside `/proc`,
`/sys`, `/dev`, `/run` and `/var/log` changed.

What still happens as a side effect of logging in over SSH, as with any SSH
login:

- The host's login bookkeeping records the session (`wtmp`, `lastlog`, the
  journal or syslog).
- On Ubuntu, `pam_motd` creates `~/.cache/motd.legal-displayed` in the
  monitoring account's home directory on its first login.

Exceptions you control:

- **dnf/yum**: `dnf` and `yum` always write their own log files and, for
  unprivileged users, a cache under `/var/tmp`. Pending updates on
  RHEL-family hosts are therefore only listed when
  `collect.dnf_updates = true`.
- **apt as root**: if you monitor with the root account (not recommended),
  `apt-get -s` may refresh its binary package cache in `/var/cache/apt`.
- **Actions** run exactly the commands an administrator configured, which
  may change the host. They are disabled unless configured.

## Creating the monitoring account

On every monitored host:

```sh
# Unprivileged account with a home directory for authorized_keys.
useradd --create-home --shell /bin/sh monitor
install -d -m 700 -o monitor -g monitor ~monitor/.ssh

# Restrict the key: no PTY, no forwarding, only from the helmsight server.
echo 'restrict,from="10.0.0.5" ssh-ed25519 AAAA... helmsight' \
  > ~monitor/.ssh/authorized_keys
chown monitor:monitor ~monitor/.ssh/authorized_keys
chmod 600 ~monitor/.ssh/authorized_keys
```

`restrict` disables port, agent and X11 forwarding and PTY allocation.
helmsight needs none of them. Replace `10.0.0.5` with the address of the
helmsight server.

Do not give the account a password; it should only log in with the key.
Setting `PasswordAuthentication no` in `sshd_config` on your hosts is also
recommended.

Optional group memberships, each adding visibility:

| Group | Gives | Notes |
|---|---|---|
| `systemd-journal` (or `adm` on Debian/Ubuntu) | failed SSH login history | read-only access to logs |
| `docker` | container list and resource usage | **equivalent to root**; only grant if you accept that |

Without these, helmsight shows "n/a" with the reason in the relevant view.

The SSH key used by helmsight should be dedicated to it and of type
ed25519 (`ssh-keygen -t ed25519 -f /etc/helmsight/id_ed25519 -N ""`). RSA
keys work, but the RSA implementation used has a known timing side channel
in private-key operations (RUSTSEC-2023-0071), so prefer ed25519. Keys can be loaded
from files (`ssh.identity_files`, readable only by the helmsight user) or
from ssh-agent. Passphrase-protected keys must be loaded into ssh-agent.

### Trusting host keys

New hosts are refused until their key is trusted. Either:

- run `helmsight hosts test --trust`, which shows each fingerprint and asks
  for confirmation; or
- approve the key in the UI under **Host keys**. The administrator must
  paste the fingerprint printed by `ssh-keygen -lf
  /etc/ssh/ssh_host_ed25519_key.pub` on the host, and approval only succeeds
  if it matches the key presented.

Approved keys are stored in `<data_dir>/known_hosts`. Existing OpenSSH
files can be consulted read-only with `ssh.known_hosts_files`. If you
already distribute your hosts' keys with a configuration management tool,
pointing helmsight at that file is the safest option.

## Actions

Actions are the only feature that changes monitored hosts, and they are off
unless configured.

- Commands are written by an administrator in the configuration file. The
  UI cannot pass arguments and there is no templating.
- Every action must list its target hosts, groups or tags explicitly.
- The UI shows the exact command and host in a confirmation dialog.
- Privileged commands must use `sudo -n`. Grant exactly those commands in
  sudoers ([examples/sudoers](https://github.com/enderkus/helmsight/blob/main/examples/sudoers)); never grant shells,
  editors, wildcards or `ALL`.
- Every run is written to the audit log twice: before it starts and with its
  exit status and truncated output.
- The audit log is append-only. Database triggers reject updates and
  deletes, and each entry carries a SHA-256 hash chained to the previous
  one; `helmsight audit verify` (or **Verify integrity** in the UI) detects
  tampering.

## The central server

- Run helmsight as a dedicated system user with the hardened systemd unit in
  [examples/helmsight.service](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.service).
- The data directory is created with mode 0700; the database, key file and
  self-signed TLS key with mode 0600. helmsight refuses to use a key file
  readable by other users.
- Secrets in the configuration can be references (`secret:<name>`,
  `env:<VAR>`, `file:<path>`) instead of plain text. Stored secrets and TOTP
  seeds are encrypted with XChaCha20-Poly1305 using `<data_dir>/secret.key`,
  created on first run. Back the key file up separately from the database.
- helmsight never logs secrets, passwords, session tokens or command output
  of collections.

## Web application security

- Binds to `127.0.0.1` by default. On other addresses, TLS is enabled
  automatically, with provided certificates or a self-signed one whose
  SHA-256 fingerprint is logged at start-up. Plain HTTP on a public address
  requires `server.tls.allow_insecure_http = true`, meant for deployments
  behind a TLS-terminating reverse proxy (set `server.public_url` to its
  `https://` URL and `server.trusted_proxies` to its address).
- Passwords are hashed with Argon2id (19 MiB, 2 iterations); unknown users
  take the same time as wrong passwords, so user names cannot be guessed
  from response times. The minimum length is 12 characters.
- Optional TOTP (RFC 6238); codes cannot be reused. Admins can be required
  to enrol (`auth.require_totp_for_admins`).
- OpenID Connect uses the authorization code flow with PKCE, state and
  nonce. Roles come from a configurable claim; users without a mapped role
  are denied unless `default_role` is set.
- Session cookies are random 256-bit tokens, stored hashed, `HttpOnly`,
  `SameSite=Strict`, `Secure` with TLS and `__Host-` prefixed. Sessions
  expire after `auth.session_ttl` and after `auth.idle_timeout` of
  inactivity.
- State-changing requests need the per-session `X-CSRF-Token` header and are
  rejected when the `Origin` header names another site.
- Sign-in is rate limited per user name and per client address.
- Responses carry `Content-Security-Policy` (no inline code, no framing),
  `X-Content-Type-Options`, `Referrer-Policy: no-referrer`,
  `Cross-Origin-Opener-Policy`, `Permissions-Policy` and HSTS when TLS is
  used. API responses are not cached.
- Wall display tokens are stored hashed, grant read access to a fixed set of
  groups only, are passed in the URL fragment (never sent to servers or
  logs) and can be revoked at any time.
- The Prometheus endpoint is disabled unless a bearer token is configured.

## Security checklist before deployment

- [ ] helmsight runs as a dedicated system user, with the systemd unit or
      in a container.
- [ ] The monitoring key is an ed25519 key used only by helmsight, and its
      file is not readable by others.
- [ ] The `authorized_keys` line has `restrict,from="…"`.
- [ ] The monitoring account is not in the `docker` group (or was added
      knowing that this is equivalent to root).
- [ ] Host keys were approved after comparing fingerprints;
      `accept_new_host_keys` is only enabled on a trusted network.
- [ ] The UI is on loopback behind a TLS proxy, or served directly with
      TLS; `public_url` is set.
- [ ] TOTP is required for administrators
      (`auth.require_totp_for_admins = true`), or sign-in uses SSO.
- [ ] Secrets are written as `secret:`, `env:` or `file:` references; the
      configuration file contains no plain-text passwords.
- [ ] `secret.key` is backed up separately from the database.
- [ ] If actions are used, sudoers only allows the exact commands.

## Reporting a vulnerability

Please report security problems privately via GitHub's **Report a
vulnerability** button on the repository's Security tab. Do not open a
public issue. Include the version (`helmsight --version`), a description,
and steps to reproduce. Reports can be written in English or Turkish.

We aim to acknowledge reports within three working days and to publish a
fix and advisory as soon as possible. We credit reporters unless they prefer
otherwise.
