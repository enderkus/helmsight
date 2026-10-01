# FAQ

**Why not an agent?**
Agents must be installed, updated, configured and trusted on every host,
and they widen the attack surface. SSH is already there, already hardened
and already audited. helmsight adds one read-only login session.

**How much load does it put on a host?**
One short shell script every 5 seconds that reads files under `/proc` and
runs a few lightweight tools: a few milliseconds of CPU. Expensive checks
such as pending updates run every 6 hours. The intervals can be changed in
the [`[collect]`](configuration.md#collect) section.

**How many resources does the central server need?**
helmsight is a single process; it keeps one persistent SSH session per
host and compact rows of metrics. A few hundred hosts on a small VM are
expected to be fine.

**Does it need root?**
No. Use a dedicated unprivileged account. A few details need group
memberships (journal access for failed logins, the Docker socket for
containers); helmsight shows "n/a" with the reason for anything it cannot
read.

**Does it really write nothing on the hosts?**
helmsight's commands write nothing; an integration test checks this on six
distributions. SSH logins are recorded by the host as usual (wtmp, lastlog,
the journal, and on Ubuntu a one-time `pam_motd` marker in the account's
home directory). `dnf` and `yum` always write log files, so pending updates
on RHEL-family hosts are only listed with `collect.dnf_updates = true`.

**Which distributions are supported?**
Debian, Ubuntu, RHEL and its rebuilds (Rocky, Alma), Fedora, openSUSE and
Alpine. Any Linux with an SSH server and a POSIX shell should work; missing
tools degrade to "n/a".

**Can I monitor non-Linux systems (BSD, macOS, Windows)?**
No. The collection script relies on Linux's `/proc` file system.

**Can I use a jump host (bastion)?**
Not yet. Hosts with `ProxyJump` in an imported SSH configuration are
skipped with a warning.

**How many hosts can it handle?**
Collection is concurrent with one persistent session per host; the number
of hosts collected at the same time is limited by `collect.max_parallel`.
Storage uses one compact row per host per sample.

**Where is the data?**
In the data directory: `helmsight.db` (SQLite), `secret.key`,
`known_hosts` and `tls/`. Back up the key file separately from the
database; stored secrets cannot be decrypted without it. See
[Installation → Backups](installation.md#backups).

**Can I export the data?**
Yes: there is a JSON API with an OpenAPI document, a Prometheus endpoint
and webhooks for alerts. See [API and integrations](api.md).

**Is there a dark mode?**
Yes. The UI follows the OS preference, and you can override it in the top
bar.

**Is the UI available in other languages?**
Not yet; the UI is in English. The documentation is available in English
and [Turkish](https://enderkus.github.io/helmsight/tr/docs/).

**Can I use it in production?**
helmsight is at version 0.x: it is tested, but it has had little
production use and no independent security audit. Try it on non-critical
hosts first, read the [security model](security.md), and keep in mind that
you use it at your own risk.

**I found a bug or a vulnerability.**
Report bugs through [GitHub Issues](https://github.com/enderkus/helmsight/issues).
Report vulnerabilities privately, without opening a public issue, as
described in the [security model](security.md#reporting-a-vulnerability).
