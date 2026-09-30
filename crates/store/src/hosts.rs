//! Host ids and the latest slow-changing data per host.

use crate::{Result, Store};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Returns the id for a host name, creating it on first use.
    pub async fn host_id(&self, name: &str) -> Result<i64> {
        let name = name.to_string();
        self.write(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO hosts(name, created_at) VALUES (?1, ?2)",
                params![name, crate::now()],
            )?;
            Ok(
                c.query_row("SELECT id FROM hosts WHERE name = ?1", [&name], |r| {
                    r.get(0)
                })?,
            )
        })
        .await
    }

    /// Stores the latest JSON value of `kind` (medium, inventory, updates ...).
    pub async fn put_latest(&self, host_id: i64, kind: &str, ts: i64, json: String) -> Result<()> {
        let kind = kind.to_string();
        self.write(move |c| {
            c.execute(
                "INSERT INTO host_latest(host_id, kind, ts, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(host_id, kind) DO UPDATE SET ts = excluded.ts, data = excluded.data",
                params![host_id, kind, ts, json],
            )?;
            Ok(())
        })
        .await
    }

    /// Returns `(ts, json)` for the latest value of `kind`.
    pub async fn get_latest(&self, host_id: i64, kind: &str) -> Result<Option<(i64, String)>> {
        let kind = kind.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                "SELECT ts, data FROM host_latest WHERE host_id = ?1 AND kind = ?2",
                params![host_id, kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })
        .await
    }

    /// All latest values of `kind`, by host id.
    pub async fn all_latest(&self, kind: &str) -> Result<Vec<(i64, i64, String)>> {
        let kind = kind.to_string();
        self.read(move |c| {
            let mut stmt =
                c.prepare("SELECT host_id, ts, data FROM host_latest WHERE kind = ?1")?;
            let rows = stmt
                .query_map([kind], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }
}
