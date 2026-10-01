# Quick start

## Try it on one machine

On a Linux machine, local mode monitors the machine itself by reading
`/proc` directly. No SSH and no configuration file are needed:

```sh
./helmsight serve --local
```

helmsight prints a one-time setup link:

```text
  No users exist yet. Create the first administrator:

    http://127.0.0.1:8080/setup#token=…
```

Open it, choose a user name and a password (at least 12 characters), and
you are in. Data is kept in `~/.local/share/helmsight` (or
`/var/lib/helmsight` when running as root).

Local mode reads `/proc`, so it only works on Linux. On other platforms,
monitor servers over SSH as described below.

## Monitor servers over SSH

```sh
./helmsight init                    # configuration file, key file, first admin
$EDITOR helmsight.toml              # add [[hosts]] entries
./helmsight hosts test --trust      # check SSH and confirm host keys
./helmsight serve
```

Before `hosts test`, create the monitoring account on each server; see
[Monitoring a fleet](fleet-setup.md).

Open `http://127.0.0.1:8080` and sign in with the administrator you created
during `init`.

## Reaching the UI from other machines

helmsight listens on `127.0.0.1` by default. To serve other machines,
either:

- set `listen = "0.0.0.0:8443"` in `[server]`. TLS is enabled
  automatically: provide a certificate with `[server.tls] mode = "files"`,
  or let helmsight generate a self-signed one, whose fingerprint is logged
  at start-up; or
- keep the loopback address and put a TLS-terminating reverse proxy in
  front, setting `public_url` and `trusted_proxies`.
