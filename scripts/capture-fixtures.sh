#!/bin/sh
# Regenerate parser fixtures from real distributions.
#
# Builds the test containers in tests/docker, boots them (with systemd where the
# distribution uses it), produces some realistic activity (failed SSH logins, a
# failed unit) and runs the collection script both as root and as an
# unprivileged user. Output is written to crates/collect/tests/fixtures.
#
# Usage: scripts/capture-fixtures.sh [distro...]
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/crates/collect/tests/fixtures"
mkdir -p "$out"
distros=${*:-"debian ubuntu rocky fedora opensuse alpine"}

script=$(sed -e 's/__NONCE__/fixture/' \
  -e 's/__GROUPS__/metrics basics medium inventory auth updates/' \
  -e 's/__SINCE__/0/' -e 's/__OPTS__/dnf/' "$root/crates/collect/src/remote.sh")

for d in $distros; do
  img="helmsight-test-$d"
  name="helmsight-fixture-$d"
  echo "==> $d"
  docker build -q -t "$img" "$root/tests/docker/$d" >/dev/null
  docker rm -f "$name" >/dev/null 2>&1 || true
  if [ "$d" = alpine ]; then
    docker run -d --name "$name" "$img" >/dev/null
  else
    docker run -d --name "$name" --privileged --cgroupns=host \
      -v /sys/fs/cgroup:/sys/fs/cgroup:rw --tmpfs /run --tmpfs /tmp "$img" >/dev/null
  fi
  sleep 8
  docker exec "$name" sh -c '
    command -v systemctl >/dev/null && systemctl start broken.service 2>/dev/null
    for u in admin root oracle; do
      sshpass -p wrong ssh -o StrictHostKeyChecking=no -o PreferredAuthentications=password \
        -o NumberOfPasswordPrompts=1 "$u@127.0.0.1" true 2>/dev/null
    done
    ssh -o BatchMode=yes -o StrictHostKeyChecking=no ghost@127.0.0.1 true 2>/dev/null
    true'
  sleep 1
  printf '%s\n' "$script" | docker exec -i "$name" sh -s > "$out/$d-root.txt"
  printf '%s\n' "$script" | docker exec -i -u monitor "$name" sh -s > "$out/$d-user.txt"
  docker rm -f "$name" >/dev/null
done
echo "fixtures written to $out"
