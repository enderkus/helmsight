# Command line

```text
helmsight [--config <path>] [--data-dir <dir>] [--log-format text|json] <command>
```

| Command | Description |
|---|---|
| `serve [--local] [--listen ADDR]` | Run the web server and collectors. `--local` monitors this machine only. |
| `init [--force]` | Create a configuration file, the data directory, the key file and the first administrator |
| `user add <name> [--role viewer\|operator\|admin] [--password-stdin]` | Create a local user |
| `user remove <name>` | Delete a user; the last administrator cannot be removed |
| `user reset-password <name> [--password-stdin] [--reset-totp]` | Set a new password and end the user's sessions |
| `user list` | List users |
| `hosts test [--trust] [names...]` | Check connectivity, authentication and host keys |
| `config check` | Validate the configuration and check that referenced secrets exist |
| `secret set <name> [--stdin]` | Store an encrypted secret, referenced as `secret:<name>` |
| `secret list` / `secret delete <name>` | List or delete stored secrets |
| `audit verify` | Verify the audit log hash chain |

Run `helmsight <command> --help` for detailed help on any command.

## Finding the configuration file

The configuration file is found via `--config`, `$HELMSIGHT_CONFIG`,
`/etc/helmsight/helmsight.toml` or `./helmsight.toml`. Without a
configuration file (local mode), `--data-dir` or the local-mode default
directory (`~/.local/share/helmsight`, or `/var/lib/helmsight` for root) is
used.

## Common examples

```sh
# Create a user with the password supplied by a script (e.g. automation)
printf '%s\n' "$PASSWORD" | helmsight user add alice --role operator --password-stdin

# A user who lost their TOTP device
helmsight user reset-password alice --reset-totp

# Test only two hosts
helmsight hosts test web-1 db-1

# Store the Slack webhook URL encrypted
helmsight secret set slack-webhook

# Read a secret from a file
helmsight secret set smtp --stdin < /run/secrets/smtp

# Run temporarily on another address
helmsight serve --listen 127.0.0.1:9090
```

`--password-stdin` and `--stdin` keep passwords out of the shell history
and the process list; on purpose, there is no way to pass a password as a
command-line argument.

## Exit status

Commands exit with `0` on success and a non-zero status on errors.
`hosts test` fails when at least one host cannot be reached, `config check`
when the configuration has errors, and `audit verify` when the chain is
broken, so these commands can be used as checks in scripts and CI
pipelines.

## Logging

Logging is controlled with `HELMSIGHT_LOG`, for example
`HELMSIGHT_LOG=debug` or `HELMSIGHT_LOG=info,server=debug`.
`--log-format json` writes logs as JSON lines, suitable for a log
collection system. Logs never contain secrets, passwords, session tokens
or collected output.
