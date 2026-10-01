# Installation

helmsight is a single static binary. It runs on the central machine only;
monitored hosts need nothing but an SSH server and a POSIX shell
(`/bin/sh`).

## Requirements

| Side | Requirement |
|---|---|
| Central machine | Linux, x86_64 or aarch64. The binary is static, so the glibc version does not matter; it also runs in Docker. |
| Monitored hosts | An SSH server, `/bin/sh` and an unprivileged account dedicated to monitoring. No extra packages. |
| Network | The central machine reaches the hosts' SSH port. Hosts never connect to the central machine. |
| Browser | A current Firefox, Chrome, Edge or Safari. |

Disk usage grows with the number of hosts and the retention periods, which
are set in the [`[retention]`](configuration.md#retention) section.

## Installation script

On a Linux server with systemd, the installation script sets everything up
in one go:

```sh
curl -fsSLO https://enderkus.github.io/helmsight/install.sh
less install.sh            # read it before running it as root
sudo sh install.sh
```

It asks a few questions (web UI address, external URL, account on the
monitored hosts, this server's address, first administrator and password),
shows what it is going to do and asks for confirmation. Then it:

1. downloads the release archive for this machine's architecture and
   verifies its SHA-256 checksum against `SHA256SUMS`; it stops on a
   mismatch;
2. installs `/usr/local/bin/helmsight` and creates the `helmsight` system
   user;
3. runs `helmsight init`, which writes `/etc/helmsight/helmsight.toml`,
   creates the SSH key `/etc/helmsight/id_ed25519`, the data directory
   `/var/lib/helmsight` and the first administrator;
4. installs and starts the hardened systemd service;
5. prints the address of the UI and the command that prepares each
   monitored host (see [Monitoring a fleet](fleet-setup.md)).

It does not touch the firewall, TLS certificates or a reverse proxy, and it
never overwrites an existing configuration, key or database.

| Option | Meaning |
|---|---|
| `--version <tag>` | Release to install, e.g. `v0.2.0` (default: the latest) |
| `--listen <addr>` | Address and port of the web UI (default `127.0.0.1:8080`) |
| `--public-url <url>` | External `https://` URL of the UI |
| `--ssh-user <user>` | Account on the monitored hosts (default `monitor`) |
| `--admin <name>` | First administrator (default `admin`) |
| `--from <addr>` | This server's address as the monitored hosts see it |
| `--archive <file>` / `--sums <file>` | Install from a downloaded archive and its `SHA256SUMS` (offline) |
| `--no-service` | Do not install the systemd service |
| `-y`, `--yes` | Do not ask; use the options and defaults |
| `--dry-run` | Only show what would be done |
| `--uninstall` | Remove the binary and the service; keep configuration and data |

With `--yes`, no password is asked for: the service prints a one-time setup
link for the first administrator, and the script shows it at the end.
This also works in automation such as cloud-init:

```sh
sh install.sh --yes --listen 0.0.0.0:8443 --public-url https://monitor.example.com --from 10.0.0.5
```

Running the script again on an installed machine upgrades the binary,
restarts the service and keeps the configuration and data. Without systemd
(for example on Alpine), it installs the binary and the configuration and
tells you how to start helmsight.

## Prebuilt binaries

Releases provide static Linux binaries for x86_64 and aarch64, with
SHA-256 checksums:

```sh
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/helmsight-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
tar xzf helmsight-x86_64-unknown-linux-musl.tar.gz
sudo install -m 0755 helmsight-x86_64-unknown-linux-musl/helmsight /usr/local/bin/
helmsight --version
```

On ARM servers (for example AWS Graviton, or a Raspberry Pi 4/5 running
64-bit Linux), replace `x86_64` with `aarch64`.

The `sha256sum --check` line verifies that the archive is intact and
identical to the file on the release page; do not continue until it
prints `OK`.

## Container image

```sh
docker run -d --name helmsight -p 8443:8080 \
  -v helmsight:/var/lib/helmsight \
  -v /etc/helmsight:/etc/helmsight:ro \
  ghcr.io/enderkus/helmsight
```

In the container, set `listen = "0.0.0.0:8080"` and
`data_dir = "/var/lib/helmsight"` in `/etc/helmsight/helmsight.toml`.
Because it listens on a non-loopback address, TLS is enabled
automatically: unless you provide a certificate, a self-signed one is
generated and its fingerprint is logged at start-up. Open
`https://<machine>:8443` in the browser.

The image contains only the binary and CA certificates (not even a
shell). `--local` mode is therefore not available in it, and commands run
as `docker exec helmsight /helmsight <command>`, for example:

```sh
docker exec -it helmsight /helmsight user add alice --role admin
docker exec -it helmsight /helmsight hosts test --trust
```

Keep the SSH key and the configuration under `/etc/helmsight`, mounted
read-only, and the database in a named volume (`helmsight`). The image runs
as an unprivileged user with UID 65532; mounted files must be readable by
that user.

## Building from source

Requirements: Rust (stable), Node.js 22 and npm.

```sh
git clone https://github.com/enderkus/helmsight.git
cd helmsight
(cd web && npm ci && npm run build)
cargo build --release
./target/release/helmsight --version
```

The web UI is embedded into the binary at compile time, so build it in
`web` first. A `cargo build` without a built UI prints a warning and embeds
a simple placeholder page instead.

To build a static Linux binary on another platform (for example macOS),
build the container image and copy the binary out of it:

```sh
docker build --platform linux/amd64 -t helmsight .
docker create --name hs helmsight
docker cp hs:/helmsight ./helmsight && docker rm hs
```

## Running as a service

The repository ships a hardened systemd unit,
[`examples/helmsight.service`](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.service):

```sh
# A dedicated system user
sudo useradd --system --home-dir /var/lib/helmsight --shell /usr/sbin/nologin helmsight

# Configuration and SSH key: owned by root, readable by the helmsight group
sudo install -d -m 0750 -o root -g helmsight /etc/helmsight
sudo install -m 0640 -o root -g helmsight helmsight.toml /etc/helmsight/
sudo install -m 0640 -o root -g helmsight id_ed25519 /etc/helmsight/

sudo cp helmsight.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now helmsight
journalctl -u helmsight -f
```

The unit runs helmsight as an unprivileged user with a read-only file
system, a private state directory (`/var/lib/helmsight`) and a restricted
set of system calls. Use `data_dir = "/var/lib/helmsight"` in the
configuration.

To create the first user, either open the setup link printed in the
service log, or run:

```sh
sudo -u helmsight helmsight --config /etc/helmsight/helmsight.toml user add alice --role admin
```

## Upgrading

1. Read the [changelog](changelog.md) of the new release; 0.x releases may
   contain incompatible changes.
2. Back up the data directory (see below).
3. Install the new binary over the old one and restart the service:
   `sudo systemctl restart helmsight`. The database schema is migrated
   automatically at start-up when needed.

With the container image, pull the new image and recreate the container
with the same volumes.

## Backups

The data directory contains:

| File | Contents |
|---|---|
| `helmsight.db` | Metrics, inventory, users, alerts, audit log |
| `secret.key` | Key that encrypts stored secrets and TOTP seeds |
| `known_hosts` | Approved SSH host keys |
| `tls/` | Self-signed certificate (if one was generated) |

For a consistent database backup, stop the service briefly and copy the
files, or use SQLite's backup command while it runs:

```sh
sqlite3 /var/lib/helmsight/helmsight.db ".backup '/backup/helmsight.db'"
```

Store `secret.key` **separately** from the database: without it, stored
secrets cannot be decrypted, and anyone who obtains both can read them.

## Uninstalling

helmsight installs nothing on monitored hosts, so uninstalling only touches
the central machine: stop the service and delete the binary,
`/etc/helmsight` and the data directory. You can also remove the
monitoring account and its `authorized_keys` line from the hosts.
