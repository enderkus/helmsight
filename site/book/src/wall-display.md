# Wall display

![Wall display](images/display.png)

The wall display is a full-screen, read-only view for TV screens: large
status tiles grouped by host group, firing alerts and a clock, without
navigation. When the fleet does not fit, it rotates through the groups
every 20 seconds.

Open `/display` while signed in, or use a display token so the screen does
not need an account:

1. As an administrator, open **Wall display** and create a token, choosing
   the groups the screen may see (or all hosts).
2. Copy the address shown once, for example
   `https://monitor.example.com/display#token=hsd_…`, and open it on the
   screen.

The token is in the URL fragment, so it is never sent to web servers or
written to their logs. It grants read access to the chosen groups only, and
can be revoked at any time; the screen then shows that the token is no
longer valid.

Options in the fragment: `rotate=<seconds>` changes the rotation interval,
`theme=dark` or `theme=light` overrides the colour scheme.
