# Contributing

Thank you for helping. This document covers the development setup, the
checks every change must pass and the rules that keep helmsight safe.

## Development setup

Requirements: Rust stable (see `rust-toolchain.toml`), Node.js 22, npm and,
for integration tests, Docker.

```sh
(cd web && npm ci && npm run build)   # the binary embeds web/dist
cargo build
./target/debug/helmsight serve --local --data-dir /tmp/helmsight-dev
```

For UI work, run the backend and the Vite dev server side by side; Vite
proxies `/api` to `http://127.0.0.1:8080` (override with
`HELMSIGHT_DEV_BACKEND`):

```sh
cargo run -- serve --config dev.toml
(cd web && npm run dev)
```

Without a built UI, `cargo build` embeds a placeholder page and prints a
warning, so Rust-only work does not need Node.js.

## Project layout

| Path | Contents |
|---|---|
| `crates/collect` | The remote POSIX script (`src/remote.sh`), parsers for its output and rate computation. Pure, no I/O. |
| `crates/transport` | SSH sessions (russh), host key verification, local execution |
| `crates/store` | SQLite schema, metrics with rollups, inventory diffs, alerts, users, audit log |
| `crates/alerts` | Rule evaluation and notification channels |
| `crates/common` | Configuration schema and validation, secrets, roles, rule expressions |
| `crates/server` | HTTP API, authentication, collection engine and background workers |
| `crates/helmsight` | The command-line binary |
| `web` | Svelte and TypeScript UI |
| `tests/docker` | Container images used for fixtures and integration tests |
| `scripts` | Maintenance scripts |

## Checks

Every commit must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
(cd web && npm run check && npm run lint && npm test && npm run build)
```

Integration tests start sshd containers and verify collection end to end,
including that nothing is written on the monitored host:

```sh
HELMSIGHT_DOCKER_TESTS=1 cargo test -p transport --test docker
# all six distributions:
HELMSIGHT_DOCKER_TESTS=1 HELMSIGHT_DOCKER_DISTROS="debian ubuntu rocky fedora opensuse alpine" \
  cargo test -p transport --test docker
```

## Parser fixtures

`crates/collect/tests/fixtures` holds real script output from Debian,
Ubuntu, Rocky, Fedora, openSUSE and Alpine, captured as root and as an
unprivileged user. After changing `remote.sh`, regenerate them:

```sh
scripts/capture-fixtures.sh            # all distributions
scripts/capture-fixtures.sh alpine     # just one
```

Review the diff of the fixtures; they are part of the test suite.

## Rules for code

- **Remote output is untrusted.** Parsers must accept arbitrary bytes, never
  panic (the `collect` crate denies `unwrap`, `panic` and indexing), cap the
  size of what they keep and fall back to "n/a" with a reason. Add a test
  with real output and let the property tests in `tests/fuzz.rs` cover it.
- **The remote script is read-only.** POSIX `sh` only, no temporary files,
  every redirection to `/dev/null` (a unit test enforces this), every tool
  optional. Check it with `shellcheck -s sh`.
- **No arbitrary commands.** Never add an API that accepts a command,
  arguments or a path to run on a host.
- **Never render remote data as HTML.** No `{@html}`, no `innerHTML`; ESLint
  rejects both. Keep the UI compatible with the strict CSP: no inline
  scripts or `style="..."` attributes (use classes or `style:` directives).
- **Never log secrets**, passwords, tokens or collected output.
- **UI copy** is short, factual and technical. Errors say what happened and
  what to do next. Status is never conveyed by color alone.

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `feat(collect): parse /proc/pressure`, `fix(ui): keep table header sticky`.
- Keep commits small and buildable; every commit passes the checks above.
- Update `CHANGELOG.md` under "Unreleased" for user-visible changes, and
  the documentation when behaviour or configuration changes.

## Renaming the project

The product name lives in three places: the binary crate name in
`crates/helmsight/Cargo.toml` (and its `[[bin]]` name), `PRODUCT_NAME` in
`crates/common/src/lib.rs` (cookie names, database file name, prefixes of
Prometheus metrics) and `PRODUCT_NAME` in `web/src/lib/brand.ts`.

## Reporting security issues

Please do not open public issues for vulnerabilities; see
[docs/security.md](docs/security.md#reporting-a-vulnerability).
