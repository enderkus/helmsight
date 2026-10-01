# Monitoring a fleet

This page walks through bringing a fleet of servers under monitoring, step
by step. In the examples, the machine running helmsight has the address
`10.0.0.5` and the monitoring account is called `monitor`.

## 1. Create a key for helmsight

If you used the [installation script](installation.md#installation-script)
or `helmsight init --generate-key`, the key already exists
(`/etc/helmsight/id_ed25519`); continue with step 2.

On the machine that runs helmsight (the commands assume the `helmsight`
system user from [Installation](installation.md#running-as-a-service)
exists):

```sh
sudo install -d -m 0750 -o root -g helmsight /etc/helmsight
sudo ssh-keygen -t ed25519 -f /etc/helmsight/id_ed25519 -N "" -C helmsight
sudo chgrp helmsight /etc/helmsight/id_ed25519
sudo chmod 0640 /etc/helmsight/id_ed25519
```

Use an ed25519 key dedicated to helmsight. RSA keys work too, but the RSA
library in use has a known timing side channel, so prefer ed25519 (details
in the [security model](security.md#creating-the-monitoring-account)).
Passphrase-protected keys must be loaded into ssh-agent
(`ssh.use_agent = true`).

## 2. Create the monitoring account on each host

### With `helmsight hosts bootstrap`

helmsight prints a script that does everything below on a host: it creates
the account, installs the public key with `restrict,from=`, unlocks key
logins on Alpine and adds the account to the journal group. Running it
again is safe.

```sh
helmsight hosts bootstrap --from 10.0.0.5 | ssh root@web-1 sh
```

`--from` is the helmsight server's address as the host sees it (several
addresses or networks can be separated by commas, e.g.
`10.0.0.5,10.1.0.0/16`). Without it, the key is accepted from anywhere and
a warning is printed. To check the script before running it, save it to a
file:

```sh
helmsight hosts bootstrap --from 10.0.0.5 > monitor-account.sh
```

| Option | Meaning |
|---|---|
| `--from <addr>` | Addresses the key is accepted from |
| `--user <user>` | Account to create (default: `ssh.user`) |
| `--key <path>` | Private key whose public key is installed (default: the first of `ssh.identity_files`) |
| `--no-journal` | Do not add the account to `systemd-journal` or `adm` |

### By hand

On every host to monitor, as root:

```sh
useradd --create-home --shell /bin/sh monitor
install -d -m 700 -o monitor -g monitor ~monitor/.ssh
echo 'restrict,from="10.0.0.5" ssh-ed25519 AAAA… helmsight' > ~monitor/.ssh/authorized_keys
chown monitor:monitor ~monitor/.ssh/authorized_keys
chmod 600 ~monitor/.ssh/authorized_keys
```

- Replace `ssh-ed25519 AAAA… helmsight` with the contents of
  `/etc/helmsight/id_ed25519.pub`.
- Replace `10.0.0.5` with the address of the helmsight server as the
  monitored host sees it; behind NAT, that is the translated address.
  Thanks to `from=`, a stolen key cannot be used from another machine.
- `restrict` disables port, agent and X11 forwarding and terminal (PTY)
  allocation. helmsight needs none of them.
- The account's shell must be a working POSIX shell (`/bin/sh`); nothing
  can be collected with `/usr/sbin/nologin`.

On Alpine, create the account with `adduser -D -s /bin/sh monitor`. Alpine
creates accounts locked (`!`), and OpenSSH refuses key logins to locked
accounts. Mark the password as unusable without locking the account:

```sh
sed -i 's/^monitor:!/monitor:*/' /etc/shadow
```

Do not use `passwd -u`: on BusyBox it can leave the account with an empty
password.

For many servers, roll these steps out with the tools you already use,
such as Ansible, Salt or cloud-init. An example Ansible task:

```yaml
- name: helmsight monitoring account
  ansible.builtin.user:
    name: monitor
    shell: /bin/sh
    create_home: true

- name: helmsight key
  ansible.posix.authorized_key:
    user: monitor
    key: "{{ lookup('file', 'files/helmsight.pub') }}"
    key_options: 'restrict,from="10.0.0.5"'
    exclusive: true
```

### Optional group memberships

Some data needs extra permissions. Without them, helmsight shows "n/a"
with the reason in the relevant view; nothing else is affected.

| Group | Enables | Note |
|---|---|---|
| `systemd-journal` (or `adm` on Debian/Ubuntu) | Failed SSH login history | Read-only access to logs |
| `docker` | Container list and resource usage | **Equivalent to root**; only grant it if you accept that |

```sh
usermod -aG systemd-journal monitor
```

Nothing else is installed on the host.

## 3. Describe your hosts

In `helmsight.toml`:

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
baseline = "web-1"    # compare drift and new ports against web-1

[[hosts]]
name = "db-1"
address = "db-1.internal"
port = 2222
user = "observer"     # a different account for this host
groups = ["db"]
```

**Choosing groups and tags.** Groups split the fleet coarsely (`web`,
`db`, `edge`) and are used as scopes in filters, alert rules, actions,
notification channels and wall display tokens. Tags are free-form and
usually written as `key:value` (`env:prod`, `dc:fra-1`, `team:payments`).
A host can have several groups and tags.

**The baseline** catches differences between servers that do the same job.
With `baseline = "web-1"` on `web-2`, the **Compare** view shows the
package, port, unit, kernel and OS differences between the two, and the
security page lists ports that listen on web-2 but not on web-1.

### Keeping the host list separate

Hosts can also live in a separate file (relative to the configuration
file):

```toml
hosts_file = "hosts.toml"
```

`hosts.toml` contains only `[[hosts]]` entries, which makes it easy to
generate the host list from an inventory tool.

### Importing from `~/.ssh/config`

```toml
[ssh_config_import]
path = "~/.ssh/config"
hosts = ["web-*", "db-*"]
tags = ["source:ssh-config"]
```

`HostName`, `User`, `Port` and `IdentityFile` are honoured. An explicit
`[[hosts]]` entry with the same name takes precedence. Hosts with
`ProxyJump` are skipped with a warning.

Validate the configuration with `helmsight config check`. Every error names
the file, line and key:

```text
error: helmsight.toml:23:1: `hosts[1].name`: duplicate host name `web-1`
```

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

Before confirming, compare the fingerprint on the host itself:

```sh
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

Keys can also be approved in the UI under **Host keys**, where the
administrator pastes the fingerprint printed on the host; approval only
succeeds if it matches the key presented. A key that changes later stops
collection and raises a critical alert until an administrator reviews it.

Where there are many hosts and the network is trusted,
`ssh.accept_new_host_keys = true` trusts keys of new hosts automatically
(like OpenSSH's `accept-new`). Changed keys are still always refused.

## 5. Start the server

```sh
helmsight serve
```

Hosts show as **Pending** for a few seconds, until their first collection.
If a host stays **Down**, **Auth failed** or **Key not trusted**, see
[Troubleshooting](troubleshooting.md).

## What helmsight reads

Each tick runs one POSIX shell script over the existing SSH session: `/proc`
files, `df`, `ps`, `ss`, `systemctl`, the package database and similar
read-only sources. The full list and the guarantees are in the
[security model](security.md#what-helmsight-runs-on-monitored-hosts).
