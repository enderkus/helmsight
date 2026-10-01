//! Shell script that prepares the monitoring account on a monitored host.

/// What the generated script sets up.
pub struct Bootstrap<'a> {
    /// Account name on the monitored host.
    pub user: &'a str,
    /// `<algorithm> <base64>` of helmsight's public key.
    pub public_key: &'a str,
    /// OpenSSH `from=` pattern list (the helmsight server's address), or none.
    pub from: Option<&'a str>,
    /// Add the account to a group that can read the system journal.
    pub journal: bool,
}

fn valid_user(u: &str) -> bool {
    let mut b = u.bytes();
    matches!(b.next(), Some(c) if c.is_ascii_lowercase() || c == b'_')
        && u.len() <= 32
        && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

fn valid_from(f: &str) -> bool {
    !f.is_empty()
        && f.len() <= 512
        && f.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".:-*?/,!_[]".contains(&c))
}

fn valid_key(k: &str) -> bool {
    let mut parts = k.split(' ');
    let (Some(algo), Some(blob), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !algo.is_empty()
        && algo
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"@.-".contains(&c))
        && !blob.is_empty()
        && blob
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c))
}

impl Bootstrap<'_> {
    /// Renders a POSIX shell script to run as root on a monitored host.
    /// Every value is validated, so the script can be embedded safely.
    pub fn render(&self) -> Result<String, String> {
        if !valid_user(self.user) {
            return Err(format!("`{}` is not a valid account name", self.user));
        }
        if !valid_key(self.public_key) {
            return Err("the public key is not in OpenSSH format".into());
        }
        let options = match self.from {
            Some(f) if valid_from(f) => format!("restrict,from=\"{f}\""),
            Some(f) => {
                return Err(format!(
                    "`{f}` is not a valid address list for `from=` (e.g. 10.0.0.5 or 10.0.0.0/24)"
                ));
            }
            None => "restrict".to_string(),
        };
        let journal = if self.journal { "1" } else { "0" };
        let name = common::PRODUCT_NAME;
        let user = self.user;
        let key = self.public_key;
        Ok(format!(
            r#"#!/bin/sh
# Prepares the {name} monitoring account on this host. Run it as root.
# It creates an unprivileged account, installs {name}'s public key with
# restrictions and, optionally, lets the account read the system journal.
# Running it again is safe.
set -eu

user='{user}'
key='{options} {key} {name}'
journal={journal}

if [ "$(id -u)" -ne 0 ]; then
  echo "error: run this script as root" >&2
  exit 1
fi

if id "$user" >/dev/null 2>&1; then
  echo "account $user exists"
else
  if command -v useradd >/dev/null 2>&1; then
    useradd --create-home --shell /bin/sh "$user"
  else
    adduser -D -s /bin/sh "$user"
  fi
  # No password login, but not locked: OpenSSH refuses key logins to
  # locked accounts on systems without PAM, such as Alpine.
  if command -v usermod >/dev/null 2>&1; then
    usermod -p '*' "$user"
  else
    sed -i "s/^$user:!*:/$user:*:/" /etc/shadow
  fi
  echo "created account $user"
fi

home=$(awk -F: -v u="$user" '$1 == u {{ print $6 }}' /etc/passwd)
group=$(id -gn "$user")
if [ -z "$home" ] || [ ! -d "$home" ]; then
  echo "error: the home directory of $user does not exist" >&2
  exit 1
fi

mkdir -p "$home/.ssh"
chown "$user:$group" "$home/.ssh"
chmod 700 "$home/.ssh"
auth="$home/.ssh/authorized_keys"
touch "$auth"
blob=$(echo "$key" | awk '{{ print $(NF - 1) }}')
if grep -qF "$blob" "$auth"; then
  echo "the {name} key is already authorized; leaving $auth unchanged"
else
  printf '%s\n' "$key" >>"$auth"
  echo "authorized the {name} key in $auth"
fi
chown "$user:$group" "$auth"
chmod 600 "$auth"

if [ "$journal" = 1 ]; then
  for g in systemd-journal adm; do
    if grep -q "^$g:" /etc/group; then
      if id -nG "$user" | tr ' ' '\n' | grep -qx "$g"; then
        :
      elif command -v usermod >/dev/null 2>&1; then
        usermod -aG "$g" "$user"
        echo "added $user to $g (read access to logs)"
      else
        addgroup "$user" "$g"
        echo "added $user to $g (read access to logs)"
      fi
      break
    fi
  done
fi

echo "done: {name} can now monitor this host as $user"
"#
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB4=";

    fn script(user: &str, key: &str, from: Option<&str>) -> Result<String, String> {
        Bootstrap {
            user,
            public_key: key,
            from,
            journal: true,
        }
        .render()
    }

    #[test]
    fn renders_restricted_key_line() {
        let s = script("monitor", KEY, Some("10.0.0.5,192.168.1.0/24")).unwrap();
        assert!(s.contains(&format!(
            "key='restrict,from=\"10.0.0.5,192.168.1.0/24\" {KEY} helmsight'"
        )));
        assert!(s.contains("user='monitor'"));
        let s = script("monitor", KEY, None).unwrap();
        assert!(s.contains(&format!("key='restrict {KEY} helmsight'")));
    }

    #[test]
    fn rejects_values_that_could_break_out_of_the_script() {
        assert!(script("mon'itor", KEY, None).is_err());
        assert!(script("Monitor", KEY, None).is_err());
        assert!(script("monitor", KEY, Some("10.0.0.5\" evil")).is_err());
        assert!(script("monitor", KEY, Some("1.2.3.4'; reboot")).is_err());
        assert!(script("monitor", "ssh-ed25519 AAAA'x", None).is_err());
        assert!(script("monitor", "ssh-ed25519 AAAA extra", None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn script_is_valid_posix_sh() {
        let s = script("monitor", KEY, Some("10.0.0.5")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bootstrap.sh");
        std::fs::write(&p, s).unwrap();
        let out = std::process::Command::new("sh")
            .arg("-n")
            .arg(&p)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
