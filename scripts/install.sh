#!/bin/sh
# Installs or upgrades helmsight on this machine.
#
# Downloads a release from GitHub, verifies its SHA-256 checksum, installs
# the binary and, on a new installation, creates a system user, the
# configuration, an SSH key, the first administrator and a systemd service.
# It shows what it is going to do and asks before changing anything.
# Run `sh install.sh --help` for the options.
set -eu

repo="enderkus/helmsight"
name=helmsight
bin=/usr/local/bin/$name
etc=/etc/$name
conf=$etc/$name.toml
key=$etc/id_ed25519
data=/var/lib/$name
unit=/etc/systemd/system/$name.service

usage() {
  cat <<EOF
Usage: sh install.sh [options]

Installs or upgrades $name. A new installation gets a system user, the
configuration $conf, the SSH key $key,
the data directory $data and a systemd service.
An existing installation only gets a new binary; its configuration and
data are kept.

Options:
  --version <tag>      release to install, e.g. v0.2.0 (default: latest)
  --listen <addr>      address and port of the web UI (default: 127.0.0.1:8080)
  --public-url <url>   external https:// URL of the UI, if any
  --ssh-user <user>    account on the monitored hosts (default: monitor)
  --admin <name>       first administrator (default: admin)
  --from <addr>        this server's address as the monitored hosts see it
  --archive <file>     install from a downloaded release archive
  --sums <file>        SHA256SUMS file for --archive
  --no-service         do not install the systemd service
  -y, --yes            do not ask; use the options and defaults
  --dry-run            only show what would be done
  --uninstall          remove the binary and the service, keep data
  -h, --help           show this help
EOF
}

say() { printf '%s\n' "$*"; }
step() { printf '==> %s\n' "$*"; }
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

version=latest
listen=""
public_url=""
ssh_user=""
admin=""
from=""
archive=""
sums=""
service=1
yes=0
dry_run=0
uninstall=0

need_arg() { [ $# -ge 2 ] && [ -n "$2" ] || die "$1 needs a value"; }
while [ $# -gt 0 ]; do
  case "$1" in
    --version) need_arg "$@"; version=$2; shift 2 ;;
    --listen) need_arg "$@"; listen=$2; shift 2 ;;
    --public-url) need_arg "$@"; public_url=$2; shift 2 ;;
    --ssh-user) need_arg "$@"; ssh_user=$2; shift 2 ;;
    --admin) need_arg "$@"; admin=$2; shift 2 ;;
    --from) need_arg "$@"; from=$2; shift 2 ;;
    --archive) need_arg "$@"; archive=$2; shift 2 ;;
    --sums) need_arg "$@"; sums=$2; shift 2 ;;
    --no-service) service=0; shift ;;
    -y | --yes) yes=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown option $1 (see --help)" ;;
  esac
done

# ---------------------------------------------------------------- checks

[ "$(uname -s)" = Linux ] || die "$name runs on Linux only"
if [ "$(id -u)" -ne 0 ] && [ "$dry_run" = 0 ]; then
  die "run this script as root (or with --dry-run to see what it would do)"
fi
case "$version" in
  latest | v[0-9]*) ;;
  *) die "--version must look like v0.2.0" ;;
esac
case "$version" in *[!A-Za-z0-9.-]*) die "--version must look like v0.2.0" ;; esac
if [ -n "$archive" ] || [ -n "$sums" ]; then
  [ -n "$archive" ] && [ -n "$sums" ] || die "--archive and --sums must be used together"
  [ -f "$archive" ] || die "$archive not found"
  [ -f "$sums" ] || die "$sums not found"
fi

case "$(uname -m)" in
  x86_64 | amd64) target=x86_64-unknown-linux-musl ;;
  aarch64 | arm64) target=aarch64-unknown-linux-musl ;;
  *) die "unsupported architecture $(uname -m); build from source instead" ;;
esac
file=$name-$target.tar.gz

has_systemd=0
if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
  has_systemd=1
fi
[ "$has_systemd" = 1 ] || service=0

tty=0
if [ "$yes" = 0 ]; then
  if (: </dev/tty) 2>/dev/null; then
    tty=1
  else
    die "no terminal to ask questions on; run with --yes and options"
  fi
fi

ask() { # question default -> REPLY
  if [ "$tty" = 1 ]; then
    if [ -n "$2" ]; then
      printf '%s [%s]: ' "$1" "$2" >/dev/tty
    else
      printf '%s: ' "$1" >/dev/tty
    fi
    IFS= read -r REPLY </dev/tty || REPLY=""
    [ -n "$REPLY" ] || REPLY=$2
  else
    REPLY=$2
  fi
}

confirm() { # question -> 0 for yes
  [ "$yes" = 1 ] && return 0
  printf '%s [y/N]: ' "$1" >/dev/tty
  IFS= read -r a </dev/tty || a=""
  case "$a" in y | Y | yes | YES) return 0 ;; *) return 1 ;; esac
}

restore_tty() { [ "$tty" = 1 ] && stty echo </dev/tty 2>/dev/null || true; }

read_secret() { # prompt -> REPLY
  printf '%s' "$1" >/dev/tty
  stty -echo </dev/tty
  IFS= read -r REPLY </dev/tty || REPLY=""
  stty echo </dev/tty
  printf '\n' >/dev/tty
}

# ---------------------------------------------------------------- uninstall

if [ "$uninstall" = 1 ]; then
  say "This removes:"
  [ -f "$unit" ] && say "  - the systemd service $unit"
  [ -e "$bin" ] && say "  - the binary $bin"
  say "and keeps the configuration $etc, the data $data and the user $name."
  [ "$dry_run" = 1 ] && exit 0
  confirm "Continue?" || die "cancelled"
  if [ -f "$unit" ]; then
    systemctl disable --now "$name" >/dev/null 2>&1 || true
    rm -f "$unit"
    systemctl daemon-reload
    step "removed the service"
  fi
  rm -f "$bin"
  step "removed $bin"
  say ""
  say "To remove everything else, review and run:"
  say "  rm -rf $etc $data && userdel $name"
  exit 0
fi

# ---------------------------------------------------------------- answers

fresh=1
[ -f "$conf" ] && fresh=0

installed=""
[ -x "$bin" ] && installed=$("$bin" --version 2>/dev/null || true)

detect_address() {
  a=""
  if command -v ip >/dev/null 2>&1; then
    a=$(ip -o route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([^ ]*\).*/\1/p' | head -n 1)
  fi
  if [ -z "$a" ] && command -v hostname >/dev/null 2>&1; then
    a=$(hostname -I 2>/dev/null | awk '{ print $1 }')
  fi
  printf '%s' "$a"
}

password=""
if [ "$fresh" = 1 ]; then
  [ "$tty" = 1 ] && say "$name installation. Press Enter to accept the value in brackets." && say ""
  ask "Address and port of the web UI" "${listen:-127.0.0.1:8080}"
  listen=$REPLY
  ask "External https:// URL of the UI (empty for none)" "$public_url"
  public_url=$REPLY
  ask "Account on the monitored hosts" "${ssh_user:-monitor}"
  ssh_user=$REPLY
  ask "This server's address as the monitored hosts see it" "${from:-$(detect_address)}"
  from=$REPLY
  ask "First administrator" "${admin:-admin}"
  admin=$REPLY

  case "$ssh_user" in "" | *[!a-z0-9_-]*) die "invalid account name: $ssh_user" ;; esac
  case "$admin" in "" | *[!A-Za-z0-9._@-]*) die "invalid administrator name: $admin" ;; esac
  case "$listen" in "" | *[!]0-9A-Za-z.:[-]*) die "invalid address: $listen" ;; esac
  case "$public_url" in "" | https://*) ;; *) die "--public-url must start with https://" ;; esac

  if [ "$tty" = 1 ] && [ "$dry_run" = 0 ]; then
    trap restore_tty EXIT INT TERM
    while :; do
      read_secret "Password for $admin (at least 12 characters): "
      password=$REPLY
      if [ "${#password}" -lt 12 ]; then
        say "The password must be at least 12 characters."
        continue
      fi
      read_secret "Repeat the password: "
      [ "$REPLY" = "$password" ] && break
      say "The passwords do not match."
    done
  fi
fi

# ---------------------------------------------------------------- plan

if [ "$version" = latest ]; then
  source_desc="the latest release of github.com/$repo"
else
  source_desc="release $version of github.com/$repo"
fi

say ""
say "Plan:"
if [ -n "$archive" ]; then
  say "  - verify the SHA-256 checksum of $archive"
else
  say "  - download $file from $source_desc and verify its SHA-256 checksum"
fi
if [ -n "$installed" ]; then
  say "  - replace $bin ($installed)"
else
  say "  - install $bin"
fi
if [ "$fresh" = 1 ]; then
  id "$name" >/dev/null 2>&1 || say "  - create the system user $name"
  say "  - create $conf (listen $listen${public_url:+, public URL $public_url}, SSH user $ssh_user)"
  [ -e "$key" ] || say "  - create the SSH key $key"
  say "  - create the data directory $data"
  if [ -n "$password" ]; then
    say "  - create the administrator $admin"
  else
    say "  - leave the first administrator to the setup link printed by the service"
  fi
else
  say "  - keep the existing configuration $conf and data"
fi
if [ "$service" = 1 ]; then
  if [ -f "$unit" ]; then
    say "  - restart the systemd service $name"
  else
    say "  - install and start the systemd service $name"
  fi
elif [ "$has_systemd" = 0 ]; then
  say "  - no systemd found: you start $name yourself"
fi
say ""

if [ "$dry_run" = 1 ]; then
  say "Dry run: nothing was changed."
  exit 0
fi
confirm "Continue?" || die "cancelled"

# ---------------------------------------------------------------- download

tmp=$(mktemp -d)
cleanup() {
  rm -rf "$tmp"
  rm -f "$bin.new"
  restore_tty
}
trap cleanup EXIT INT TERM

fetch() { # url dest
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "curl or wget is required"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  else
    die "sha256sum or shasum is required to verify the download"
  fi
}

if [ -n "$archive" ]; then
  cp "$archive" "$tmp/$file"
  cp "$sums" "$tmp/SHA256SUMS"
else
  if [ "$version" = latest ]; then
    base="https://github.com/$repo/releases/latest/download"
  else
    base="https://github.com/$repo/releases/download/$version"
  fi
  step "downloading $file"
  fetch "$base/$file" "$tmp/$file" || die "download failed: $base/$file"
  fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "download failed: $base/SHA256SUMS"
fi

expected=$(awk -v f="$file" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "SHA256SUMS has no entry for $file"
actual=$(sha256 "$tmp/$file")
[ "$actual" = "$expected" ] || die "checksum mismatch for $file (expected $expected, got $actual)"
step "checksum verified"

mkdir "$tmp/x"
tar -xzf "$tmp/$file" -C "$tmp/x"
pkg="$tmp/x/$name-$target"
[ -f "$pkg/$name" ] || die "the archive does not contain $name-$target/$name"
# Stage the binary next to its destination: /tmp is often mounted noexec.
mkdir -p "$(dirname "$bin")"
install -m 0755 "$pkg/$name" "$bin.new"
new_version=$("$bin.new" --version) || die "the downloaded binary does not run on this machine"
if [ "$fresh" = 1 ] && ! "$bin.new" init --help 2>/dev/null | grep -q -- --non-interactive; then
  die "$new_version cannot be set up by this script; install $name 0.2.0 or later"
fi

# ---------------------------------------------------------------- install

if ! id "$name" >/dev/null 2>&1; then
  nologin=$(command -v nologin 2>/dev/null || echo /bin/false)
  if command -v useradd >/dev/null 2>&1; then
    useradd --system --user-group --home-dir "$data" --no-create-home --shell "$nologin" "$name"
  else
    addgroup -S "$name"
    adduser -S -D -H -h "$data" -s "$nologin" -G "$name" "$name"
  fi
  step "created the system user $name"
fi

was_active=0
if [ "$has_systemd" = 1 ] && systemctl is-active --quiet "$name" 2>/dev/null; then
  was_active=1
fi

mv -f "$bin.new" "$bin"
step "installed $bin ($new_version)"

if [ "$fresh" = 1 ]; then
  install -d -m 0750 -o root -g "$name" "$etc"
  set -- --config "$conf" --data-dir "$data" init --non-interactive \
    --listen "$listen" --ssh-user "$ssh_user" --ssh-key "$key" --generate-key \
    --admin "$admin"
  [ -n "$public_url" ] && set -- "$@" --public-url "$public_url"
  if [ -n "$password" ]; then
    printf '%s\n' "$password" | "$bin" "$@" --password-stdin >"$tmp/init.log" 2>&1 ||
      { cat "$tmp/init.log" >&2; die "$name init failed"; }
  else
    "$bin" "$@" >"$tmp/init.log" 2>&1 </dev/null ||
      { cat "$tmp/init.log" >&2; die "$name init failed"; }
  fi
  password=""
  chgrp "$name" "$conf" "$key" "$key.pub"
  chmod 0640 "$conf" "$key"
  chown -R "$name:$name" "$data"
  chmod 0700 "$data"
  step "created $conf, $key and $data"
fi

if [ "$service" = 1 ]; then
  if [ ! -f "$unit" ]; then
    [ -f "$pkg/examples/$name.service" ] || die "the archive does not contain examples/$name.service"
    install -m 0644 "$pkg/examples/$name.service" "$unit"
    systemctl daemon-reload
    systemctl enable --quiet "$name"
    step "installed the systemd service"
  fi
  if [ "$was_active" = 1 ] || [ "$fresh" = 1 ] || ! systemctl is-active --quiet "$name"; then
    systemctl restart "$name"
  fi
  n=0
  until systemctl is-active --quiet "$name"; do
    n=$((n + 1))
    [ "$n" -gt 15 ] && die "the service did not start; see: journalctl -u $name"
    sleep 1
  done
  step "service $name is running"
fi

# ---------------------------------------------------------------- summary

if [ "$fresh" = 0 ]; then
  say ""
  say "Upgraded to $new_version. Configuration and data were kept."
  exit 0
fi

case "$listen" in
  127.* | localhost:* | \[::1\]:*) url="http://$listen" ;;
  0.0.0.0:*) url="https://$(detect_address):${listen##*:}" ;;
  *) url="https://$listen" ;;
esac
[ -n "$public_url" ] && url=$public_url

setup=""
if ! grep -q "Administrator .* created" "$tmp/init.log"; then
  if [ "$service" = 1 ]; then
    n=0
    while [ -z "$setup" ] && [ "$n" -lt 10 ]; do
      setup=$(journalctl -u "$name" -o cat --no-pager 2>/dev/null |
        grep -o 'https\{0,1\}://[^ ]*/setup#token=[A-Za-z0-9_-]*' | tail -n 1 || true)
      [ -n "$setup" ] || sleep 1
      n=$((n + 1))
    done
  fi
fi

pubkey=$(cat "$key.pub")
bootstrap="$name hosts bootstrap --config $conf${from:+ --from $from}"

cat <<EOF

$new_version is installed.

  Web UI          $url
EOF
if [ -n "$setup" ]; then
  say "  First login     open $setup"
elif [ "$service" = 0 ]; then
  say "  Start it        su -s /bin/sh $name -c '$bin serve --config $conf'"
  say "                  (it prints a setup link if no administrator exists)"
else
  say "  Sign in as      $admin"
fi
say "  Configuration   $conf"
say "  Data            $data"
[ "$service" = 1 ] && say "  Logs            journalctl -u $name -f"
cat <<EOF

Next steps:

  1. Prepare each monitored host. This creates the unprivileged account
     $ssh_user with $name's public key, accepted only from ${from:-any address}:

       $bootstrap | ssh root@<host> sh

     The public key is:
       $pubkey

  2. Add the hosts to $conf, for example:

       [[hosts]]
       name = "web-1"
       address = "10.0.0.11"

  3. Check them and confirm their host keys (as the $name user, so that
     the service can read the approved keys), then restart the service:

       su -s /bin/sh $name -c '$name hosts test --config $conf --trust'
       systemctl restart $name

Documentation: https://enderkus.github.io/$name/
EOF
