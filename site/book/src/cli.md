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

The configuration file is found via `--config`, `$HELMSIGHT_CONFIG`,
`/etc/helmsight/helmsight.toml` or `./helmsight.toml`. Without a
configuration file (local mode), `--data-dir` or the local default
directory is used.

Logging is controlled with `HELMSIGHT_LOG`, for example
`HELMSIGHT_LOG=debug` or `HELMSIGHT_LOG=info,server=debug`.
