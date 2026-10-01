# Installation

helmsight is a single static binary. It runs on the central machine only;
monitored hosts need nothing but an SSH server and a POSIX shell.

## Prebuilt binaries

Releases provide static Linux binaries for x86_64 and aarch64, with
checksums:

```sh
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/helmsight-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
tar xzf helmsight-x86_64-unknown-linux-musl.tar.gz
sudo install -m 0755 helmsight-x86_64-unknown-linux-musl/helmsight /usr/local/bin/
```

Replace `x86_64` with `aarch64` on ARM servers.

## Container image

```sh
docker run -d --name helmsight -p 8443:8080 \
  -v helmsight:/var/lib/helmsight \
  -v /etc/helmsight:/etc/helmsight:ro \
  ghcr.io/enderkus/helmsight
```

In the container, set `listen = "0.0.0.0:8080"` and
`data_dir = "/var/lib/helmsight"` in `/etc/helmsight/helmsight.toml`. TLS is
then enabled automatically with a self-signed certificate unless you provide
one. The image contains only the binary and CA certificates, so `--local`
mode is not available in it.

## Building from source

Requirements: Rust (stable), Node.js 22 and npm.

```sh
git clone https://github.com/enderkus/helmsight.git
cd helmsight
(cd web && npm ci && npm run build)
cargo build --release
./target/release/helmsight --version
```

The web UI is embedded into the binary at compile time. To build a static
Linux binary on another platform, build the container image and copy the
binary out of it:

```sh
docker build --platform linux/amd64 -t helmsight .
docker create --name hs helmsight
docker cp hs:/helmsight ./helmsight && docker rm hs
```

## Running as a service

The repository ships a hardened systemd unit,
[`examples/helmsight.service`](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.service):

```sh
sudo useradd --system --home-dir /var/lib/helmsight --shell /usr/sbin/nologin helmsight
sudo install -d -m 0750 -o root -g helmsight /etc/helmsight
sudo install -m 0640 -o root -g helmsight helmsight.toml /etc/helmsight/
sudo install -m 0640 -o root -g helmsight id_ed25519 /etc/helmsight/
sudo cp helmsight.service /etc/systemd/system/
sudo systemctl enable --now helmsight
journalctl -u helmsight -f
```

The unit runs helmsight as an unprivileged user with a read-only file
system, a private state directory and a restricted set of system calls.
