//! Append-only audit log. Rows cannot be updated or deleted (enforced by
//! triggers), and each row carries a SHA-256 hash chained to the previous
//! row, so tampering with the database file is detectable.

use crate::{Result, Store, StoreError};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: i64,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub detail: serde_json::Value,
    pub hash: String,
}

fn chain_hash(
    prev: &str,
    ts: i64,
    actor: &str,
    action: &str,
    target: Option<&str>,
    detail: &str,
) -> String {
    let mut h = Sha256::new();
    for part in [
        prev,
        &ts.to_string(),
        actor,
        action,
        target.unwrap_or(""),
        detail,
    ] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    hex::encode(h.finalize())
}

/// Result of verifying the hash chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainStatus {
    pub entries: i64,
    /// First entry whose hash does not match, if any.
    pub broken_at: Option<i64>,
}

impl Store {
    pub async fn audit(
        &self,
        actor: &str,
        action: &str,
        target: Option<&str>,
        detail: serde_json::Value,
    ) -> Result<i64> {
        let actor = actor.to_string();
        let action = action.to_string();
        let target = target.map(str::to_string);
        self.write(move |c| {
            let tx = c.transaction()?;
            let prev: String = tx
                .query_row(
                    "SELECT hash FROM audit_log ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_default();
            let ts = crate::now();
            let detail =
                serde_json::to_string(&detail).map_err(|e| StoreError::Other(e.to_string()))?;
            let hash = chain_hash(&prev, ts, &actor, &action, target.as_deref(), &detail);
            tx.execute(
                "INSERT INTO audit_log(ts, actor, action, target, detail, prev_hash, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![ts, actor, action, target, detail, prev, hash],
            )?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(id)
        })
        .await
    }

    /// Entries newest first. `before` pages backwards by id.
    pub async fn audit_entries(
        &self,
        before: Option<i64>,
        action_prefix: Option<String>,
        limit: u32,
    ) -> Result<Vec<AuditEntry>> {
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, ts, actor, action, target, detail, hash FROM audit_log
                 WHERE (?1 IS NULL OR id < ?1) AND (?2 IS NULL OR action LIKE ?2 || '%')
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(params![before, action_prefix, limit], |r| {
                    let detail: String = r.get(5)?;
                    Ok(AuditEntry {
                        id: r.get(0)?,
                        ts: r.get(1)?,
                        actor: r.get(2)?,
                        action: r.get(3)?,
                        target: r.get(4)?,
                        detail: serde_json::from_str(&detail).unwrap_or(serde_json::Value::Null),
                        hash: r.get(6)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn verify_audit_chain(&self) -> Result<ChainStatus> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, ts, actor, action, target, detail, prev_hash, hash FROM audit_log ORDER BY id",
            )?;
            let mut rows = stmt.query([])?;
            let mut prev = String::new();
            let mut n = 0;
            while let Some(r) = rows.next()? {
                n += 1;
                let id: i64 = r.get(0)?;
                let target: Option<String> = r.get(4)?;
                let expected = chain_hash(
                    &prev,
                    r.get(1)?,
                    &r.get::<_, String>(2)?,
                    &r.get::<_, String>(3)?,
                    target.as_deref(),
                    &r.get::<_, String>(5)?,
                );
                let stored_prev: String = r.get(6)?;
                let hash: String = r.get(7)?;
                if stored_prev != prev || hash != expected {
                    return Ok(ChainStatus {
                        entries: n,
                        broken_at: Some(id),
                    });
                }
                prev = hash;
            }
            Ok(ChainStatus {
                entries: n,
                broken_at: None,
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn append_only_and_chained() {
        let store = Store::memory().unwrap();
        store
            .audit(
                "alice",
                "action.run",
                Some("web-1"),
                serde_json::json!({"exit": 0}),
            )
            .await
            .unwrap();
        store
            .audit("bob", "user.create", Some("carol"), serde_json::json!({}))
            .await
            .unwrap();
        let st = store.verify_audit_chain().await.unwrap();
        assert_eq!(
            st,
            ChainStatus {
                entries: 2,
                broken_at: None
            }
        );

        let upd = store
            .write(|c| Ok(c.execute("UPDATE audit_log SET actor = 'mallory'", [])?))
            .await;
        assert!(upd.is_err(), "updates must be rejected");
        let del = store
            .write(|c| Ok(c.execute("DELETE FROM audit_log", [])?))
            .await;
        assert!(del.is_err(), "deletes must be rejected");

        let list = store
            .audit_entries(None, Some("action.".into()), 10)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].detail["exit"], 0);

        // Tampering that bypasses triggers is detected.
        store
            .write(|c| {
                c.execute_batch(
                    "DROP TRIGGER audit_log_no_update; UPDATE audit_log SET actor = 'mallory' WHERE id = 1;",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(store.verify_audit_chain().await.unwrap().broken_at, Some(1));
    }
}
