# FAQ

**Why not an agent?**
Agents must be installed, updated, configured and trusted on every host,
and they widen the attack surface. SSH is already there, already hardened
and already audited. helmsight adds one read-only login session.

**How much load does it put on a host?**
One short shell script every 5 seconds that reads files under `/proc` and
runs a few lightweight tools: a few milliseconds of CPU. Expensive checks
such as pending updates run every 6 hours.

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

**Can I use a jump host?**
Not yet. Hosts with `ProxyJump` in an imported SSH configuration are
skipped with a warning.

**How many hosts can it handle?**
Collection is concurrent with one persistent session per host. Storage uses
one compact row per host per sample. A few hundred hosts on a small VM are
expected to be fine.

**Where is the data?**
In the data directory: `helmsight.db` (SQLite), `secret.key`,
`known_hosts` and `tls/`. Back up the key file separately from the
database; stored secrets cannot be decrypted without it.
