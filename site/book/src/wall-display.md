# Wall display

![Wall display](images/display.png)

The wall display is a full-screen, read-only view for TV and NOC screens:
large status tiles grouped by host group, firing alerts and a clock,
without navigation. When the fleet does not fit, it rotates through the
groups every 20 seconds. Status is shown with text and icons, not only
colour, so it stays readable from a distance and for people with colour
vision deficiencies.

Open `/display` while signed in, or use a display token so the screen does
not need an account:

1. As an administrator, open **Wall display** and create a token, choosing
   the groups the screen may see (or all hosts).
2. Copy the address, which is shown only once, for example
   `https://monitor.example.com/display#token=hsd_…`, and open it on the
   screen.

The token is in the URL fragment (the part after `#`), so it is never sent
to web servers or written to their logs. It grants read access to the
chosen groups only, and can be revoked at any time; the screen then shows
that the token is no longer valid.

Tokens do not expire on their own: the same address keeps working after
the screen's browser restarts. If a screen is stolen or changes hands,
revoke only that screen's token; other screens are not affected. Creating
one token per screen is therefore a good habit.

## Options

Append options to the fragment with `&`:

| Option | Meaning |
|---|---|
| `rotate=<seconds>` | Time between groups (default 20) |
| `theme=dark` / `theme=light` | Force the colour scheme |

Example: `https://monitor.example.com/display#token=hsd_…&rotate=30&theme=dark`

## Tips for setting up a screen

- Open the browser in kiosk mode, for example
  `chromium --kiosk --noerrdialogs "https://monitor.example.com/display#token=…"`.
- Turn off the screen's sleep mode and screen saver.
- With a self-signed certificate, make the screen's browser trust it;
  otherwise the warning page appears after every restart.
- When the connection drops, the screen shows that its data is stale and
  recovers on its own when the connection returns.
