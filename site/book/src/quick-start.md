# Quick start

This page shows two ways to get helmsight running in a few minutes: trying
it on one machine without SSH, and monitoring real servers over SSH.

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

The token is in the part of the address after `#`; browsers do not send
that part to the server, so it is not written to any log. While no user
exists, helmsight prints a new link at every start.

Local mode reads `/proc`, so it only works on Linux. On other platforms,
monitor servers over SSH as described below.

## Monitor servers over SSH

Four steps:

```sh
./helmsight init                    # configuration file, key file, first admin
$EDITOR helmsight.toml              # add [[hosts]] entries
./helmsight hosts test --trust      # check SSH and confirm host keys
./helmsight serve
```

1. **`init`** asks a few questions (listen address, data directory, SSH
   user, path of the SSH private key, name and password of the first
   administrator). It then writes a commented `helmsight.toml` and creates
   the data directory and `secret.key`, which encrypts stored secrets. It
   does not generate the SSH key; the next page explains how.
2. **Add the hosts** to monitor in `helmsight.toml`:

   ```toml
   [[hosts]]
   name = "web-1"
   address = "10.0.0.11"
   groups = ["web"]
   ```

3. **`hosts test --trust`** connects to every host, shows the fingerprint
   of host keys it has not seen before and asks you to confirm them, then
   prints a one-line result per host. Create the monitoring account on each
   server first; see [Monitoring a fleet](fleet-setup.md).
4. **`serve`** starts the web server and the collectors.

Open `http://127.0.0.1:8080` and sign in with the administrator you created
during `init`. Hosts show as **Pending** for a few seconds, until their
first collection.

## Reaching the UI from other machines

helmsight listens on `127.0.0.1` by default, so the UI is only reachable
from the same machine. There are two ways to open it to others:

**1. Listen directly.** Set `listen = "0.0.0.0:8443"` in `[server]`. TLS is
enabled automatically: provide your own certificate with
`[server.tls] mode = "files"`, or let helmsight generate a self-signed one.
The fingerprint of a generated certificate is logged at start-up; compare
it before accepting the browser's warning.

```toml
[server]
listen = "0.0.0.0:8443"
public_url = "https://monitor.example.com:8443"

[server.tls]
mode = "files"
cert = "/etc/helmsight/tls/fullchain.pem"
key = "/etc/helmsight/tls/privkey.pem"
```

**2. Behind a reverse proxy.** helmsight stays on the loopback address and
a proxy such as nginx, Caddy or Traefik terminates TLS. Set `public_url`
and `trusted_proxies`:

```toml
[server]
listen = "127.0.0.1:8080"
public_url = "https://monitor.example.com"
trusted_proxies = ["127.0.0.1"]
```

An example nginx configuration (turning off buffering matters for
server-sent events):

```nginx
server {
    listen 443 ssl;
    server_name monitor.example.com;
    # ssl_certificate ... ; ssl_certificate_key ... ;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

`public_url` is required for OpenID Connect; it is also used in links in
notifications and by the origin check that refuses requests from other
sites.

## Next steps

- [Monitoring a fleet](fleet-setup.md): the monitoring account, the host
  list and host key approval.
- [Alerts and notifications](alerts.md): rules such as disk usage, and
  Slack or email notifications.
- [Security model](security.md): exactly what helmsight runs on your
  servers.
