//! Time series storage with automatic rollups.
//!
//! Each tick of each host is stored as one row whose blob contains all
//! series values. Rollups aggregate raw rows into 1-minute and 1-minute
//! rows into 5-minute rows (average, minimum and maximum).

use crate::{Result, Store, StoreError};
use collect::model::Metrics;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Turns metrics into `(series key, value)` pairs.
pub fn flatten(m: &Metrics) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = Vec::with_capacity(64);
    let mut push = |k: String, x: f64| {
        if x.is_finite() {
            v.push((k, x));
        }
    };
    if let Some(cpu) = &m.cpu {
        let t = &cpu.total;
        push("cpu.busy".into(), t.busy);
        push("cpu.user".into(), t.user + t.nice);
        push("cpu.system".into(), t.system);
        push("cpu.iowait".into(), t.iowait);
        push("cpu.steal".into(), t.steal);
        push("cpu.irq".into(), t.irq + t.softirq);
        for (i, c) in cpu.per_core.iter().enumerate() {
            push(format!("cpu.core:{i}"), *c);
        }
    }
    if let Some(mem) = &m.mem {
        push("mem.used_pct".into(), mem.used_pct);
        push("mem.used".into(), mem.used as f64);
        push("mem.cached".into(), (mem.cached + mem.buffers) as f64);
        push("mem.available".into(), mem.available as f64);
        push("mem.total".into(), mem.total as f64);
        push("swap.used_pct".into(), mem.swap_used_pct);
        push("swap.used".into(), mem.swap_used as f64);
    }
    if let Some(l) = &m.load {
        push("load.1".into(), l.one);
        push("load.5".into(), l.five);
        push("load.15".into(), l.fifteen);
    }
    for f in &m.filesystems {
        push(format!("fs.used_pct:{}", f.mount), f.used_pct);
        push(format!("fs.used:{}", f.mount), f.used as f64);
        push(format!("fs.total:{}", f.mount), f.total as f64);
        if let Some(p) = f.inodes_used_pct {
            push(format!("fs.inodes_pct:{}", f.mount), p);
        }
    }
    for d in &m.disks {
        push(format!("disk.read:{}", d.device), d.read_bps);
        push(format!("disk.write:{}", d.device), d.write_bps);
        push(format!("disk.util:{}", d.device), d.util_pct);
        push(
            format!("disk.iops:{}", d.device),
            d.read_iops + d.write_iops,
        );
    }
    for n in &m.net {
        push(format!("net.rx:{}", n.name), n.rx_bps);
        push(format!("net.tx:{}", n.name), n.tx_bps);
        push(format!("net.errors:{}", n.name), n.rx_errors + n.tx_errors);
    }
    if let Some(t) = &m.tcp {
        push("tcp.established".into(), t.established as f64);
        push("tcp.time_wait".into(), t.time_wait as f64);
        push("tcp.total".into(), t.total as f64);
    }
    if let Some(p) = &m.procs {
        push("procs.total".into(), f64::from(p.total));
        push("procs.running".into(), f64::from(p.running));
    }
    v.retain(|(k, _)| k.len() <= 512);
    v
}

fn encode_raw(values: &[(u32, f64)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 8);
    for (id, v) in values {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    out
}

fn decode_raw(data: &[u8]) -> impl Iterator<Item = (u32, f64)> + '_ {
    data.as_chunks::<8>().0.iter().filter_map(|c| {
        let id = u32::from_le_bytes(c.get(0..4)?.try_into().ok()?);
        let v = f32::from_le_bytes(c.get(4..8)?.try_into().ok()?);
        Some((id, f64::from(v)))
    })
}

#[derive(Debug, Clone, Copy)]
struct Agg {
    sum: f64,
    n: u32,
    min: f64,
    max: f64,
}

impl Agg {
    fn new(v: f64) -> Self {
        Agg {
            sum: v,
            n: 1,
            min: v,
            max: v,
        }
    }
    fn add(&mut self, avg: f64, min: f64, max: f64) {
        self.sum += avg;
        self.n += 1;
        self.min = self.min.min(min);
        self.max = self.max.max(max);
    }
    fn avg(&self) -> f64 {
        self.sum / f64::from(self.n.max(1))
    }
}

fn encode_rollup(values: &BTreeMap<u32, Agg>) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 16);
    for (id, a) in values {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(a.avg() as f32).to_le_bytes());
        out.extend_from_slice(&(a.min as f32).to_le_bytes());
        out.extend_from_slice(&(a.max as f32).to_le_bytes());
    }
    out
}

/// (id, avg, min, max)
fn decode_rollup(data: &[u8]) -> impl Iterator<Item = (u32, f64, f64, f64)> + '_ {
    data.as_chunks::<16>().0.iter().filter_map(|c| {
        let id = u32::from_le_bytes(c.get(0..4)?.try_into().ok()?);
        let f = |r: std::ops::Range<usize>| -> Option<f64> {
            Some(f64::from(f32::from_le_bytes(c.get(r)?.try_into().ok()?)))
        };
        Some((id, f(4..8)?, f(8..12)?, f(12..16)?))
    })
}

impl Store {
    /// Resolves series keys to ids, creating missing ones.
    fn series_ids(&self, conn: &Connection, keys: &[&str]) -> Result<Vec<u32>> {
        let mut cache = self
            .series_cache()
            .lock()
            .map_err(|_| StoreError::Other("series cache poisoned".into()))?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            if let Some(id) = cache.get(*k) {
                out.push(*id);
                continue;
            }
            conn.execute("INSERT OR IGNORE INTO series_keys(key) VALUES (?1)", [k])?;
            let id: i64 =
                conn.query_row("SELECT id FROM series_keys WHERE key = ?1", [k], |r| {
                    r.get(0)
                })?;
            let id =
                u32::try_from(id).map_err(|_| StoreError::Other("series id overflow".into()))?;
            cache.insert((*k).to_string(), id);
            out.push(id);
        }
        Ok(out)
    }

    /// Stores one sample row.
    pub async fn insert_sample(
        &self,
        host_id: i64,
        ts: i64,
        values: Vec<(String, f64)>,
    ) -> Result<()> {
        let me = self.clone();
        self.write(move |c| {
            let keys: Vec<&str> = values.iter().map(|(k, _)| k.as_str()).collect();
            let ids = me.series_ids(c, &keys)?;
            let pairs: Vec<(u32, f64)> = ids
                .into_iter()
                .zip(values.iter().map(|(_, v)| *v))
                .collect();
            c.execute(
                "INSERT OR REPLACE INTO samples_raw(host_id, ts, data) VALUES (?1, ?2, ?3)",
                params![host_id, ts, encode_raw(&pairs)],
            )?;
            Ok(())
        })
        .await
    }

    /// Aggregates completed minutes into 1-minute rows and completed
    /// 5-minute windows into 5-minute rows. Safe to call repeatedly.
    pub async fn rollup(&self, now: i64) -> Result<()> {
        self.write(move |c| {
            rollup_level(c, "1m", "samples_raw", "samples_1m", 60, now, false)?;
            rollup_level(c, "5m", "samples_1m", "samples_5m", 300, now, true)?;
            Ok(())
        })
        .await
    }

    /// Deletes data older than the configured retention (seconds).
    pub async fn apply_retention(
        &self,
        now: i64,
        raw: i64,
        minute: i64,
        five_minute: i64,
        events: i64,
    ) -> Result<usize> {
        self.write(move |c| {
            let mut n = 0;
            n += c.execute("DELETE FROM samples_raw WHERE ts < ?1", [now - raw])?;
            n += c.execute("DELETE FROM samples_1m WHERE ts < ?1", [now - minute])?;
            n += c.execute("DELETE FROM samples_5m WHERE ts < ?1", [now - five_minute])?;
            n += c.execute("DELETE FROM changes WHERE ts < ?1", [now - events])?;
            n += c.execute("DELETE FROM auth_failures WHERE ts < ?1", [now - events])?;
            n += c.execute(
                "DELETE FROM alerts WHERE state = 'resolved' AND resolved_at < ?1",
                [now - events],
            )?;
            n += c.execute("DELETE FROM notifications WHERE ts < ?1", [now - events])?;
            n += c.execute("DELETE FROM silences WHERE ends_at < ?1", [now - events])?;
            Ok(n)
        })
        .await
    }

    /// Returns the series matching `patterns` (exact keys, or prefixes ending
    /// in `*`) for a host between `from` and `to` (unix seconds), reduced to
    /// at most `max_points` points per series.
    pub async fn query(&self, q: SeriesQuery) -> Result<QueryResult> {
        let SeriesQuery {
            host_id,
            patterns,
            from,
            to,
            max_points,
            retention_raw,
            retention_minute,
        } = q;
        self.read(move |c| {
            let span = (to - from).max(1);
            let now = crate::now();
            let (table, step) = if span <= 6 * 3600 && from >= now - retention_raw {
                ("samples_raw", 0)
            } else if span <= 8 * 86_400 && from >= now - retention_minute {
                ("samples_1m", 60)
            } else {
                ("samples_5m", 300)
            };
            let mut keys: HashMap<u32, String> = HashMap::new();
            {
                let mut stmt = c.prepare_cached("SELECT id, key FROM series_keys")?;
                let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
                for row in rows {
                    let (id, key) = row?;
                    if matches_any(&key, &patterns)
                        && let Ok(id) = u32::try_from(id) {
                            keys.insert(id, key);
                        }
                }
            }
            let mut series: BTreeMap<String, Vec<(i64, f64, f64, f64)>> = BTreeMap::new();
            let sql = format!(
                "SELECT ts, data FROM {table} WHERE host_id = ?1 AND ts >= ?2 AND ts <= ?3 ORDER BY ts"
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(params![host_id, from, to])?;
            while let Some(row) = rows.next()? {
                let ts: i64 = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                if table == "samples_raw" {
                    for (id, v) in decode_raw(&data) {
                        if let Some(k) = keys.get(&id) {
                            series.entry(k.clone()).or_default().push((ts, v, v, v));
                        }
                    }
                } else {
                    for (id, avg, min, max) in decode_rollup(&data) {
                        if let Some(k) = keys.get(&id) {
                            series.entry(k.clone()).or_default().push((ts, avg, min, max));
                        }
                    }
                }
            }
            let resolution = if step == 0 { "raw" } else if step == 60 { "1m" } else { "5m" };
            Ok(QueryResult {
                resolution: resolution.to_string(),
                series: series
                    .into_iter()
                    .map(|(key, points)| Series {
                        key,
                        points: downsample(points, max_points),
                    })
                    .collect(),
            })
        })
        .await
    }
}

fn matches_any(key: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => key.starts_with(prefix),
        None => key == p,
    })
}

/// Parameters of [`Store::query`].
#[derive(Debug, Clone)]
pub struct SeriesQuery {
    pub host_id: i64,
    /// Exact keys, or prefixes ending in `*`.
    pub patterns: Vec<String>,
    pub from: i64,
    pub to: i64,
    pub max_points: usize,
    /// Retention of raw and 1-minute data, in seconds, to pick a resolution.
    pub retention_raw: i64,
    pub retention_minute: i64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Series {
    pub key: String,
    /// `[ts, avg, min, max]`
    #[schema(value_type = Vec<Vec<f64>>)]
    pub points: Vec<(i64, f64, f64, f64)>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct QueryResult {
    pub resolution: String,
    pub series: Vec<Series>,
}

/// Reduces points by averaging consecutive buckets, keeping min and max.
fn downsample(points: Vec<(i64, f64, f64, f64)>, max: usize) -> Vec<(i64, f64, f64, f64)> {
    if max == 0 || points.len() <= max {
        return points;
    }
    let per = points.len().div_ceil(max);
    points
        .chunks(per)
        .filter_map(|chunk| {
            let first = chunk.first()?;
            let n = chunk.len() as f64;
            let avg = chunk.iter().map(|p| p.1).sum::<f64>() / n;
            let min = chunk.iter().map(|p| p.2).fold(f64::INFINITY, f64::min);
            let max = chunk.iter().map(|p| p.3).fold(f64::NEG_INFINITY, f64::max);
            Some((first.0, avg, min, max))
        })
        .collect()
}

fn rollup_level(
    c: &mut Connection,
    level: &str,
    src: &str,
    dst: &str,
    bucket: i64,
    now: i64,
    src_is_rollup: bool,
) -> Result<()> {
    let done: i64 = c
        .query_row(
            "SELECT done_until FROM rollup_state WHERE level = ?1",
            [level],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| {
            let sql = format!("SELECT COALESCE(MIN(ts), 0) FROM {src}");
            c.query_row(&sql, [], |r| r.get(0)).unwrap_or(0)
        });
    let start = done - done.rem_euclid(bucket);
    // Only complete buckets; leave a margin for late samples.
    let end = (now - bucket) - (now - bucket).rem_euclid(bucket);
    if end <= start {
        return Ok(());
    }
    let tx = c.transaction()?;
    {
        let sql = format!(
            "SELECT host_id, ts, data FROM {src} WHERE ts >= ?1 AND ts < ?2 ORDER BY host_id, ts"
        );
        let mut stmt = tx.prepare(&sql)?;
        let mut rows = stmt.query(params![start, end])?;
        let mut buckets: BTreeMap<(i64, i64), BTreeMap<u32, Agg>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            let host: i64 = row.get(0)?;
            let ts: i64 = row.get(1)?;
            let data: Vec<u8> = row.get(2)?;
            let b = ts - ts.rem_euclid(bucket);
            let entry = buckets.entry((host, b)).or_default();
            if src_is_rollup {
                for (id, avg, min, max) in decode_rollup(&data) {
                    entry
                        .entry(id)
                        .and_modify(|a| a.add(avg, min, max))
                        .or_insert(Agg {
                            sum: avg,
                            n: 1,
                            min,
                            max,
                        });
                }
            } else {
                for (id, v) in decode_raw(&data) {
                    entry
                        .entry(id)
                        .and_modify(|a| a.add(v, v, v))
                        .or_insert_with(|| Agg::new(v));
                }
            }
        }
        let ins = format!("INSERT OR REPLACE INTO {dst}(host_id, ts, data) VALUES (?1, ?2, ?3)");
        let mut ins = tx.prepare(&ins)?;
        for ((host, b), values) in &buckets {
            ins.execute(params![host, b, encode_rollup(values)])?;
        }
    }
    tx.execute(
        "INSERT INTO rollup_state(level, done_until) VALUES (?1, ?2)
         ON CONFLICT(level) DO UPDATE SET done_until = excluded.done_until",
        params![level, end],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_round_trip() {
        let v = vec![(1u32, 1.5f64), (7, 1e9)];
        let back: Vec<(u32, f64)> = decode_raw(&encode_raw(&v)).collect();
        assert_eq!(back[0], (1, 1.5));
        assert!((back[1].1 - 1e9).abs() / 1e9 < 1e-6);
        assert_eq!(decode_raw(&[1, 2, 3]).count(), 0);
    }

    #[test]
    fn downsampling_keeps_extremes() {
        let pts: Vec<_> = (0..100)
            .map(|i| (i, i as f64, i as f64, i as f64))
            .collect();
        let d = downsample(pts, 10);
        assert_eq!(d.len(), 10);
        assert_eq!(d[0], (0, 4.5, 0.0, 9.0));
    }

    #[tokio::test]
    async fn insert_rollup_query() {
        let store = Store::memory().unwrap();
        let host = store.host_id("web-1").await.unwrap();
        let base = 1_700_000_040 - 1_700_000_040 % 300;
        for i in 0..120 {
            // 10 minutes of 5 s samples
            let ts = base + i * 5;
            store
                .insert_sample(
                    host,
                    ts,
                    vec![("cpu.busy".into(), (i % 12) as f64), ("load.1".into(), 1.0)],
                )
                .await
                .unwrap();
        }
        let now = base + 20 * 60;
        store.rollup(now).await.unwrap();
        let rows: i64 = store
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM samples_1m", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(rows, 10);
        let rows5: i64 = store
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM samples_5m", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(rows5, 2);
        // Idempotent.
        store.rollup(now).await.unwrap();

        let q = store
            .query(SeriesQuery {
                host_id: host,
                patterns: vec!["cpu.*".into()],
                from: base,
                to: base + 600,
                max_points: 1000,
                retention_raw: i64::MAX / 2,
                retention_minute: i64::MAX / 2,
            })
            .await
            .unwrap();
        assert_eq!(q.resolution, "raw");
        assert_eq!(q.series.len(), 1);
        assert_eq!(q.series[0].points.len(), 120);
        let m = store
            .query(SeriesQuery {
                host_id: host,
                patterns: vec!["cpu.busy".into()],
                from: base - 86_400 * 2,
                to: base + 600,
                max_points: 1000,
                retention_raw: 3600,
                retention_minute: i64::MAX / 2,
            })
            .await
            .unwrap();
        assert_eq!(m.resolution, "1m");
        let p = m.series[0].points[0];
        assert_eq!((p.1, p.2, p.3), (5.5, 0.0, 11.0));

        let deleted = store
            .apply_retention(
                base + 10 * 86_400,
                86_400,
                7 * 86_400,
                90 * 86_400,
                90 * 86_400,
            )
            .await
            .unwrap();
        assert_eq!(deleted, 130);
    }
}
