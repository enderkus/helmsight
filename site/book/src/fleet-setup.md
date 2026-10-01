# Monitoring a fleet

## 1. Create a key for helmsight

On the machine that runs helmsight:

```sh
ssh-keygen -t ed25519 -f /etc/helmsight/id_ed25519 -N "" -C helmsight
```

Use a dedicated ed25519 key. Passphrase-protected keys must be loaded into
ssh-agent (`ssh.use_agent = true`).

## 2. Create the monitoring account on each host

```sh
useradd --create-home --shell /bin/sh monitor
install -d -m 700 -o monitor -g monitor ~monitor/.ssh
echo 'restrict,from="10.0.0.5" ssh-ed25519 AAAA… helmsight' > ~monitor/.ssh/authorized_keys
chown monitor:monitor ~monitor/.ssh/authorized_keys
chmod 600 ~monitor/.ssh/authorized_keys
```

Replace `10.0.0.5` with the address of the helmsight server. `restrict`
disables forwarding and terminal allocation, which helmsight does not need.

On Alpine, create the account with `adduser -D -s /bin/sh monitor`. Alpine
creates accounts locked (`!`), and OpenSSH refuses key logins to locked
accounts. Mark the password as unusable without locking the account:

```sh
sed -i 's/^monitor:!/monitor:*/' /etc/shadow
```

Do not use `passwd -u`: on BusyBox it can leave the account with an empty
password.

Optional groups give helmsight more visibility:

| Group | Enables |
|---|---|
| `systemd-journal` (or `adm` on Debian/Ubuntu) | Failed SSH login history |
| `docker` | Container list and resource usage (**equivalent to root**) |

Without them, helmsight shows "n/a" with the reason in the relevant view.
Nothing else is installed on the host.

## 3. Describe your hosts

```toml
[ssh]
user = "monitor"
identity_files = ["/etc/helmsight/id_ed25519"]

[[hosts]]
name = "web-1"
address = "10.0.0.11"
groups = ["web"]
tags = ["env:prod", "role:web"]

[[hosts]]
name = "web-2"
address = "10.0.0.12"
groups = ["web"]
tags = ["env:prod", "role:web"]
baseline = "web-1"    # compare drift and ports against web-1
```

Hosts can also live in a separate file (`hosts_file = "hosts.toml"`) or be
imported from `~/.ssh/config`:

```toml
[ssh_config_import]
path = "~/.ssh/config"
hosts = ["web-*", "db-*"]
tags = ["source:ssh-config"]
```

Validate with `helmsight config check`. Every error names the file, line
and key.

## 4. Trust the host keys

helmsight refuses unknown host keys. Confirm them once:

```sh
helmsight hosts test --trust
```

```text
HOST        STATUS             DETAIL
web-1       host key unknown   ssh-ed25519 SHA256:ldBw6qzC… ([10.0.0.11]:22)
  Trust this key? Compare with `ssh-keygen -lf` on the host (y/N) [N]: y
  trusted; retrying
web-1       ok                 Debian GNU/Linux 12 (bookworm); user monitor (72 ms)
```

Keys can also be approved in the UI under **Host keys**, where the
administrator pastes the fingerprint printed on the host. A key that changes
later stops collection and raises a critical alert until an administrator
reviews it.

## 5. Start the server

```sh
helmsight serve
```

Hosts appear as **Pending** until their first collection, a few seconds
later.

## What helmsight reads

Each tick runs one POSIX shell script over the existing SSH session: `/proc`
files, `df`, `ps`, `ss`, `systemctl`, the package database and similar
read-only sources. The full list and the guarantees are in the
[security model](security.md#what-helmsight-runs-on-monitored-hosts).
