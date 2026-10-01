# The web interface

The UI follows the operating system's light or dark preference; switch it in
the top bar. Every view shows when its data was last updated, and stale data
is dimmed and labelled. The UI is live: host states and alerts update
through server-sent events without reloading the page.

What a user sees depends on their role:

| Role | Can |
|---|---|
| `viewer` | See all monitoring data |
| `operator` | Also acknowledge and silence alerts and run permitted actions |
| `admin` | Also manage users, host keys, wall display tokens and read the audit log |

## Fleet

![Fleet overview](images/fleet.png)

The fleet overview lists every host with its status, CPU and memory
sparklines, load, the fullest filesystem, network throughput, uptime and
firing alerts. Use it as a table or as a grid of cards.

- **Status chips** filter by state. *Needs attention* collects everything
  that is not OK, partially collected or stale.
- **Search** (`/`) matches names, addresses, operating systems, groups and
  tags; `group:web` and `tag:env:prod` match exactly.
- The statuses tell different problems apart:

| Status | Meaning |
|---|---|
| OK / Warning / Critical | Collecting; worst firing alert |
| Down | The host did not respond over SSH |
| Auth failed | The SSH server rejected the configured keys |
| Key not trusted | The host key must be approved by an administrator |
| Key changed | The host key differs from the trusted key; collection stopped |
| Pending | Waiting for the first collection |
| *partial* label | Some data could not be collected (shown with the reason) |

This distinction is deliberate: "down" is a network or hardware problem,
"auth failed" is an account or key problem, and "key changed" is a possible
security incident. Each needs a different response, often from a different
team. See [Troubleshooting](troubleshooting.md) for fixes.

## Host detail

![Host detail](images/host.png)

Click a host in the fleet list to open it. The tabs:

- **Overview**: headline values, then charts for CPU (user, system, iowait,
  steal), a per-core heatmap, memory and swap, load, disk usage per mount,
  disk I/O per device, network per interface and TCP connections. Pick a
  range from 15 minutes to 30 days or a custom one; crosshairs are
  synchronized across charts. Every chart has a table view.
- **Processes**: top processes by CPU and by memory.
- **Network**: listening ports with owning processes, interface rates and
  TCP states.
- **Services**: failed units first, then running services.
- **Containers**: Docker or Podman containers with state and usage.
- **System**: OS, kernel, hardware, logged-in users and collection details.
- **Changes**: what changed in packages, ports, units, kernel and OS.
- **Security**: failed SSH logins, pending updates and ports not present on
  the baseline.

### Reading the charts

- In the **CPU** chart, *iowait* is time spent waiting for disks and
  *steal* is time the hypervisor gave to other guests. High steal usually
  means the physical host under the virtual machine is overloaded.
- **Memory** "used" excludes reclaimable cache, so Linux using free memory
  for caching does not show up as "memory full".
- **Load** averages should be read against the number of cores; the
  `load1_per_core` metric does that for alert rules.
- Periods when a host was unreachable show as gaps; no line is drawn
  across them.
- Long ranges are drawn from the finest resolution that covers them (raw
  samples, 1-minute or 5-minute averages). On a 30-day chart, spikes of a
  few seconds are therefore smoothed out; pick a narrower range to examine
  short events.

## Compare

![Compare](images/compare.png)

Compare a host with another host, or with its configured baseline. Only
differences are shown: packages (and their versions), listening ports,
enabled units, kernel and OS release. It is the quickest way to catch
servers with the same role drifting apart over time (configuration drift).

## Security

![Security overview](images/security.png)

The fleet-wide security page brings together a chart of failed SSH logins
across all hosts over time and the count per host for the last 24 hours,
pending and security updates, reboot-required flags, ports not on each
host's baseline and TLS certificates checked from the helmsight server.

## Changes

A timeline of inventory changes across the fleet, filterable by kind and
host. This is where "what changed last night?" gets answered: installed or
upgraded packages, opened or closed ports, enabled services, a new kernel.

## Administration pages

Administrators also see:

- **Users**: local users, roles and TOTP status.
- **Host keys**: pending and changed host keys.
- **Wall display**: wall display tokens.
- **Audit log**: the tamper-evident record of actions and administrative
  changes.

Every user manages their own password and TOTP enrolment on the
**Account** page in the user menu at the top right. When actions are
configured, operators and administrators also see the **Actions** page in
the navigation.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| `/` | Focus search |
| `g` then `f` | Fleet |
| `g` then `s` | Security |
| `g` then `a` | Alerts |
| `g` then `c` | Changes |
| `?` | Show shortcuts |
| `Esc` | Close dialog or clear search |
