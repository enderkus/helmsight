# Collection script executed on monitored hosts via `sh -s`.
#
# Rules for this file:
#   * POSIX sh only (must run under dash, bash --posix and BusyBox ash).
#   * Read-only: never write files, never create temp files, never change state.
#   * Every command must tolerate being absent or not permitted.
#   * Output is a sequence of sections, each introduced by a marker line
#     "<MARKER> <name>". The marker embeds a per-run random nonce.
#
# Placeholders substituted by the server (values are generated server-side,
# never taken from user input):
#   __NONCE__   hex string
#   __GROUPS__  space separated list of group names
#   __SINCE__   unix timestamp (integer)
#   __OPTS__    space separated list of option names
hs_main() {
M='@@HS-__NONCE__@@'
HS_GROUPS=' __GROUPS__ '
SINCE='__SINCE__'
HS_OPTS=' __OPTS__ '
LC_ALL=C
LANG=C
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export LC_ALL LANG PATH

s() { printf '\n%s %s\n' "$M" "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }
want() { case $HS_GROUPS in *" $1 "*) return 0 ;; esac; return 1; }
opt() { case $HS_OPTS in *" $1 "*) return 0 ;; esac; return 1; }
rd() { [ -r "$1" ] && cat "$1" 2>/dev/null; }

TO=''
if have timeout; then
  if timeout 2 true >/dev/null 2>&1; then
    TO=new
  elif timeout -t 2 true >/dev/null 2>&1; then
    TO=old
  fi
fi
t() {
  d=$1
  shift
  case $TO in
    new) timeout "$d" "$@" ;;
    old) timeout -t "$d" "$@" ;;
    *) "$@" ;;
  esac
}

s begin
printf 'v=1\n'
printf 'groups=%s\n' "$HS_GROUPS"

if want metrics; then
  s time
  date +%s
  s stat
  rd /proc/stat
  s meminfo
  rd /proc/meminfo
  s loadavg
  rd /proc/loadavg
  s uptime
  rd /proc/uptime
  s diskstats
  rd /proc/diskstats
  s netdev
  rd /proc/net/dev
  s mounts
  rd /proc/mounts
  s df
  t 5 df -Pk 2>/dev/null
  s dfi
  t 5 df -Pi 2>/dev/null
  s tcp
  f=''
  for x in /proc/net/tcp /proc/net/tcp6; do
    [ -r "$x" ] && f="$f $x"
  done
  # shellcheck disable=SC2086
  [ -n "$f" ] && awk 'FNR>1{c[$4]++} END{for(k in c) print k, c[k]}' $f 2>/dev/null
  s procstat
  cat /proc/[0-9]*/stat 2>/dev/null
  s ps
  ps -eww -o pid=,user:32=,args= 2>/dev/null || ps -o pid,user,args 2>/dev/null
fi

if want basics; then
  s basics
  p=$(getconf PAGESIZE 2>/dev/null)
  if [ -z "$p" ]; then
    p=$(awk '/^KernelPageSize:/{print $2 * 1024; exit}' /proc/self/smaps 2>/dev/null)
  fi
  printf 'pagesize=%s\n' "$p"
  printf 'hostname=%s\n' "$(uname -n 2>/dev/null)"
  printf 'user=%s\n' "$(id -un 2>/dev/null)"
  printf 'uid=%s\n' "$(id -u 2>/dev/null)"
fi

if want medium; then
  s listen
  if have ss; then
    echo '#src ss'
    t 5 ss -Hltunp 2>/dev/null || t 5 ss -ltunp 2>/dev/null
  elif have netstat; then
    echo '#src netstat'
    t 5 netstat -ltunp 2>/dev/null || t 5 netstat -ltun 2>/dev/null
  else
    echo '#src proc'
    for x in tcp tcp6 udp udp6; do
      echo "#file $x"
      rd "/proc/net/$x"
    done
  fi

  s services
  if have systemctl && [ -d /run/systemd/system ]; then
    echo '#src systemd'
    t 10 systemctl list-units --type=service --all --no-legend --no-pager --plain 2>&1 | head -n 5000
  elif have rc-status; then
    echo '#src openrc'
    t 10 rc-status -a 2>&1 | head -n 5000
  else
    echo '#src none'
  fi

  s containers
  for rt in docker podman; do
    if have "$rt"; then
      echo "#src $rt"
      t 10 "$rt" ps -a --no-trunc --format '{{json .}}' 2>&1 | head -n 2000
      echo '#stats'
      t 15 "$rt" stats --no-stream --no-trunc --format '{{json .}}' 2>&1 | head -n 2000
      break
    fi
  done

  s who
  who 2>/dev/null | head -n 500
fi

if want inventory; then
  s osrelease
  rd /etc/os-release || rd /usr/lib/os-release

  s uname
  printf 'kernel_name=%s\n' "$(uname -s 2>/dev/null)"
  printf 'kernel_release=%s\n' "$(uname -r 2>/dev/null)"
  printf 'kernel_version=%s\n' "$(uname -v 2>/dev/null)"
  printf 'machine=%s\n' "$(uname -m 2>/dev/null)"

  s cpuinfo
  awk -F: '/^(model name|Hardware|cpu model|Processor)[[:space:]]*:/{print; exit}' /proc/cpuinfo 2>/dev/null
  have lscpu && lscpu 2>/dev/null | grep -E '^(Model name|Vendor ID|Hypervisor vendor|Virtualization type):'

  s virt
  if have systemd-detect-virt; then
    printf 'detect_virt=%s\n' "$(systemd-detect-virt 2>/dev/null)"
    printf 'detect_container=%s\n' "$(systemd-detect-virt -c 2>/dev/null)"
  fi
  [ -f /.dockerenv ] && echo 'dockerenv=1'
  [ -f /run/.containerenv ] && echo 'containerenv=1'
  printf 'dmi_vendor=%s\n' "$(rd /sys/class/dmi/id/sys_vendor)"
  printf 'dmi_product=%s\n' "$(rd /sys/class/dmi/id/product_name)"
  grep -qE '^flags.* hypervisor( |$)' /proc/cpuinfo 2>/dev/null && echo 'cpu_hypervisor=1'

  s reboot
  [ -f /var/run/reboot-required ] && echo 'flag=reboot-required'
  [ -f /run/reboot-needed ] && echo 'flag=reboot-needed'
  if [ -r /var/run/reboot-required.pkgs ]; then
    head -n 200 /var/run/reboot-required.pkgs 2>/dev/null | sed 's/^/pkg=/'
  fi
  kr=$(uname -r 2>/dev/null)
  if [ -n "$(ls /lib/modules 2>/dev/null)" ]; then
    if [ -d "/lib/modules/$kr" ] || [ -d "/usr/lib/modules/$kr" ]; then
      echo 'modules=ok'
    else
      echo "modules=missing"
    fi
  fi
  printf 'running=%s\n' "$kr"
  if have rpm; then
    # Newest installed kernel package (rpm -q is read-only).
    rpm -q --last kernel-core kernel 2>/dev/null | awk '$1 !~ /^package$/ {print "rpm_kernel=" $1; exit}'
  fi

  s packages
  if have dpkg-query; then
    echo '#src dpkg'
    # shellcheck disable=SC2016 # dpkg format fields, not shell expansions
    t 60 dpkg-query -W -f='${db:Status-Abbrev}\t${Package}\t${Version}\t${Architecture}\n' 2>/dev/null
  elif have rpm; then
    echo '#src rpm'
    t 60 rpm -qa --qf '%{NAME}\t%{EPOCH}:%{VERSION}-%{RELEASE}\t%{ARCH}\n' 2>/dev/null
  elif have apk; then
    echo '#src apk'
    t 60 apk info -v 2>/dev/null
  else
    echo '#src none'
  fi

  s units
  if have systemctl && [ -d /run/systemd/system ]; then
    echo '#src systemd'
    t 10 systemctl list-unit-files --type=service,timer,socket --state=enabled --no-legend --no-pager 2>&1 | head -n 5000
  elif have rc-update; then
    echo '#src openrc'
    t 10 rc-update show -v 2>&1 | head -n 5000
  else
    echo '#src none'
  fi
fi

if want auth; then
  s auth
  printf 'tz=%s\n' "$(date +%z 2>/dev/null)"
  printf 'now=%s\n' "$(date +%s 2>/dev/null)"
  pat='(Failed (password|publickey|keyboard-interactive|none)|Invalid user)'
  if have journalctl && [ -d /run/systemd/system ]; then
    echo '#src journal'
    t 30 journalctl --no-pager -o short-unix --since "@$SINCE" _COMM=sshd _COMM=sshd-session 2>&1 \
      | grep -E "$pat|^Hint:|No journal files|insufficient permissions|not seeing messages" | tail -n 5000
  fi
  for x in /var/log/auth.log /var/log/secure /var/log/messages; do
    if [ -f "$x" ]; then
      if [ -r "$x" ]; then
        echo "#src file $x"
        tail -n 50000 "$x" 2>/dev/null | grep -E "sshd(-session)?\\[[0-9]+\\]: .*$pat" | tail -n 5000
      else
        echo "#denied $x"
      fi
    fi
  done
fi

if want updates; then
  s updates
  if have apt-get; then
    echo '#src apt'
    n=0
    for x in /var/lib/apt/lists/*_Packages*; do
      [ -e "$x" ] && n=$((n + 1))
    done
    m=$(date -r /var/lib/apt/lists +%s 2>/dev/null)
    echo "#lists ${n:-0} ${m:-0}"
    t 120 apt-get -s -o Debug::NoLocking=true dist-upgrade 2>&1 | grep -E '^(Inst |E: )' | head -n 10000
  elif have dnf || have yum; then
    # dnf and yum always write their own log files (and, for unprivileged
    # users, a cache under /var/tmp), so they only run when enabled.
    if ! opt dnf; then
      echo '#src dnf-disabled'
    elif have dnf; then
      echo '#src dnf'
      t 120 dnf -C -q check-update 2>&1 | head -n 10000
      echo '#security'
      t 120 dnf -C -q updateinfo list --security --available 2>&1 | head -n 10000
    else
      echo '#src yum'
      t 120 yum -C -q check-update 2>&1 | head -n 10000
      echo '#security'
      t 120 yum -C -q updateinfo list security 2>&1 | head -n 10000
    fi
  elif have zypper; then
    echo '#src zypper'
    t 120 zypper -n --no-refresh -q list-updates 2>&1 | head -n 10000
    echo '#security'
    t 120 zypper -n --no-refresh -q list-patches --category security 2>&1 | head -n 10000
  elif have apk; then
    echo '#src apk'
    t 60 apk version -l '<' 2>&1 | head -n 10000
  else
    echo '#src none'
  fi
fi

s end
}
hs_main </dev/null 2>/dev/null
