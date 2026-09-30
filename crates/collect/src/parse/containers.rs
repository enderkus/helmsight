//! Containers from `docker`/`podman` `ps` and `stats` JSON lines.

use crate::model::{Container, Probe};
use crate::util::clip;
use serde_json::Value;

const MAX_CONTAINERS: usize = 1000;
const MAX_LINE: usize = 64 * 1024;

pub fn containers(text: &str) -> Probe<Vec<Container>> {
    let mut lines = text.lines();
    let Some(src) = lines.next().and_then(|l| l.strip_prefix("#src ")) else {
        return Probe::na("no container runtime found");
    };
    let runtime = match src.trim() {
        "docker" => "docker",
        "podman" => "podman",
        _ => return Probe::na("no container runtime found"),
    };
    let mut out: Vec<Container> = Vec::new();
    let mut in_stats = false;
    let mut error: Option<String> = None;
    for line in lines.take(10_000) {
        let line = line.trim();
        if line == "#stats" {
            in_stats = true;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let parsed = if line.len() <= MAX_LINE && line.starts_with('{') {
            serde_json::from_str::<Value>(line).ok()
        } else {
            None
        };
        let Some(v) = parsed else {
            if error.is_none() && !in_stats {
                error = Some(clip(line, 200));
            }
            continue;
        };
        if in_stats {
            apply_stats(&mut out, &v);
        } else if out.len() < MAX_CONTAINERS {
            out.push(from_ps(&v));
        }
    }
    if out.is_empty()
        && let Some(e) = error
    {
        return Probe::na(format!("{runtime}: {e}"));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Probe::ok(runtime, out)
}

/// Case-insensitive field lookup, since Docker and Podman differ in casing.
fn field<'a>(v: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let obj = v.as_object()?;
    names.iter().find_map(|n| {
        obj.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v)
    })
}

fn text_field(v: &Value, names: &[&str]) -> String {
    match field(v, names) {
        Some(Value::String(s)) => clip(s, 256),
        Some(Value::Array(a)) => clip(
            &a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(","),
            256,
        ),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn short_id(id: &str) -> String {
    id.chars()
        .take(12)
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn from_ps(v: &Value) -> Container {
    Container {
        id: short_id(&text_field(v, &["ID", "Id"])),
        name: text_field(v, &["Names", "Name"])
            .trim_start_matches('/')
            .to_string(),
        image: text_field(v, &["Image"]),
        state: text_field(v, &["State"]).to_ascii_lowercase(),
        status: text_field(v, &["Status"]),
        ..Container::default()
    }
}

fn apply_stats(list: &mut [Container], v: &Value) {
    let id = short_id(&text_field(v, &["ID", "Container", "Id"]));
    let name = text_field(v, &["Name", "Names"]);
    let Some(c) = list
        .iter_mut()
        .find(|c| (!id.is_empty() && c.id == id) || (!name.is_empty() && c.name == name))
    else {
        return;
    };
    c.cpu_pct = parse_pct(&text_field(v, &["CPUPerc", "cpu_percent", "CPU"]));
    c.mem_pct = parse_pct(&text_field(v, &["MemPerc", "mem_percent"]));
    let usage = text_field(v, &["MemUsage", "mem_usage"]);
    if let Some((used, limit)) = usage.split_once('/') {
        c.mem_bytes = parse_size(used);
        c.mem_limit = parse_size(limit);
    }
}

fn parse_pct(s: &str) -> Option<f64> {
    let v: f64 = s.trim().trim_end_matches('%').trim().parse().ok()?;
    v.is_finite().then_some(v.max(0.0))
}

/// Parses sizes like `10.5MiB`, `1.94GiB`, `512kB`, `0B`.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let num: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let v = num * mult;
    (v.is_finite() && (0.0..1e19).contains(&v)).then_some(v as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_ps_and_stats() {
        let text = r#"#src docker
{"Command":"\"/docker-entrypoint.…\"","CreatedAt":"2026-09-30 10:00:00 +0000 UTC","ID":"3f4e5d6c7b8a9f0e1d2c3b4a","Image":"nginx:1.27","Names":"web","State":"running","Status":"Up 2 hours"}
{"ID":"aaaaaaaaaaaa1111","Image":"redis:7","Names":"cache","State":"exited","Status":"Exited (0) 3 days ago"}
#stats
{"BlockIO":"0B / 0B","CPUPerc":"1.25%","Container":"3f4e5d6c7b8a","ID":"3f4e5d6c7b8a","MemPerc":"0.52%","MemUsage":"10.5MiB / 1.94GiB","Name":"web","NetIO":"1kB / 2kB","PIDs":"3"}
"#;
        let Probe::Ok { data, source } = containers(text) else {
            panic!()
        };
        assert_eq!(source, "docker");
        assert_eq!(data.len(), 2);
        let web = data.iter().find(|c| c.name == "web").unwrap();
        assert_eq!(web.id, "3f4e5d6c7b8a");
        assert_eq!(web.cpu_pct, Some(1.25));
        assert_eq!(web.mem_bytes, Some(11_010_048));
        assert_eq!(
            data.iter().find(|c| c.name == "cache").unwrap().state,
            "exited"
        );
    }

    #[test]
    fn podman_shapes() {
        let text = "#src podman\n{\"Id\":\"abcdef0123456789\",\"Names\":[\"db\"],\"Image\":\"postgres:16\",\"State\":\"running\",\"Status\":\"Up 5 minutes\"}\n#stats\n";
        let Probe::Ok { data, .. } = containers(text) else {
            panic!()
        };
        assert_eq!(data[0].name, "db");
        assert_eq!(data[0].id, "abcdef012345");
    }

    #[test]
    fn permission_denied() {
        let text = "#src docker\npermission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock\n#stats\npermission denied\n";
        let Probe::Na { reason } = containers(text) else {
            panic!()
        };
        assert!(reason.starts_with("docker: permission denied"));
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("512kB"), Some(512_000));
        assert_eq!(parse_size("1.5GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("--"), None);
        assert_eq!(parse_size("9e99GB"), None);
    }
}
