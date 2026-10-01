# The web interface

The UI follows the operating system's light or dark preference; switch it in
the top bar. Every view shows when its data was last updated, and stale data
is dimmed and labelled.

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

## Host detail

![Host detail](images/host.png)

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

## Compare

![Compare](images/compare.png)

Compare a host with another host, or with its configured baseline. Only
differences are shown: packages, listening ports, enabled units, kernel and
OS release.

## Security

![Security overview](images/security.png)

The fleet-wide security page aggregates failed SSH logins over time,
pending and security updates, reboot-required flags, ports not on each
host's baseline and TLS certificates checked from the helmsight server.

## Changes

A timeline of inventory changes across the fleet, filterable by kind and
host.

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
