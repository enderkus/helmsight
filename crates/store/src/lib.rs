//! Embedded SQLite storage.
//!
//! One writer connection and a few reader connections in WAL mode. All
//! access goes through [`Store::write`] and [`Store::read`], which run the
//! closure on a blocking thread.

pub mod alerts;
pub mod audit;
pub mod hosts;
pub mod inventory;
pub mod metrics;
mod migrations;
pub mod security;
pub mod users;

use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub use rusqlite;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

struct Inner {
    path: PathBuf,
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
    series_cache: Mutex<std::collections::HashMap<String, u32>>,
}

/// Handle to the database. Cheap to clone.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("path", &self.inner.path)
            .finish()
    }
}

const READERS: usize = 4;

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

impl Store {
    /// Opens (creating if needed) the database file and applies migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| StoreError::Other(format!("creating {}: {e}", dir.display())))?;
        }
        let writer = Connection::open(path)?;
        restrict_permissions(path);
        writer.pragma_update(None, "journal_mode", "WAL")?;
        configure(&writer)?;
        migrations::apply(&writer)?;
        let mut readers = Vec::with_capacity(READERS);
        for _ in 0..READERS {
            let r = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            configure(&r)?;
            readers.push(Mutex::new(r));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                path: path.to_path_buf(),
                writer: Mutex::new(writer),
                readers,
                next_reader: AtomicUsize::new(0),
                series_cache: Mutex::new(Default::default()),
            }),
        })
    }

    /// An in-memory database for tests.
    pub fn memory() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "store-test-{}-{}",
            std::process::id(),
            common::secrets::random_bytes::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        Self::open(&dir.join("test.db"))
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Runs `f` with the writer connection on a blocking thread.
    pub async fn write<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = inner
                .writer
                .lock()
                .map_err(|_| StoreError::Other("writer lock poisoned".into()))?;
            f(&mut conn)
        })
        .await
        .map_err(|e| StoreError::Other(format!("task failed: {e}")))?
    }

    /// Runs `f` with a read-only connection on a blocking thread.
    pub async fn read<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let i = inner.next_reader.fetch_add(1, Ordering::Relaxed) % inner.readers.len().max(1);
            let conn = inner
                .readers
                .get(i)
                .ok_or_else(|| StoreError::Other("no reader".into()))?
                .lock()
                .map_err(|_| StoreError::Other("reader lock poisoned".into()))?;
            f(&conn)
        })
        .await
        .map_err(|e| StoreError::Other(format!("task failed: {e}")))?
    }

    /// Synchronous access to the writer, for CLI commands.
    pub fn write_sync<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R>) -> Result<R> {
        let mut conn = self
            .inner
            .writer
            .lock()
            .map_err(|_| StoreError::Other("writer lock poisoned".into()))?;
        f(&mut conn)
    }

    pub(crate) fn series_cache(&self) -> &Mutex<std::collections::HashMap<String, u32>> {
        &self.inner.series_cache
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Current unix time in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Stored secrets, encrypted with the key file.
pub mod secrets {
    use super::*;
    use common::secrets::SecretKey;
    use rusqlite::{OptionalExtension, params};

    pub fn set(conn: &Connection, key: &SecretKey, name: &str, value: &str) -> Result<()> {
        let sealed = key.seal(value.as_bytes(), &format!("secret:{name}"));
        conn.execute(
            "INSERT INTO secrets(name, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![name, sealed, now()],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        key: &SecretKey,
        name: &str,
    ) -> Result<Option<zeroize::Zeroizing<String>>> {
        let sealed: Option<String> = conn
            .query_row("SELECT value FROM secrets WHERE name = ?1", [name], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(sealed) = sealed else {
            return Ok(None);
        };
        let plain = key
            .open(&sealed, &format!("secret:{name}"))
            .map_err(|e| StoreError::Other(e.to_string()))?;
        let s = String::from_utf8(plain.to_vec())
            .map_err(|_| StoreError::Other("secret is not valid UTF-8".into()))?;
        Ok(Some(zeroize::Zeroizing::new(s)))
    }

    pub fn delete(conn: &Connection, name: &str) -> Result<bool> {
        Ok(conn.execute("DELETE FROM secrets WHERE name = ?1", [name])? > 0)
    }

    pub fn names(conn: &Connection) -> Result<Vec<(String, i64)>> {
        let mut stmt = conn.prepare("SELECT name, updated_at FROM secrets ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

/// Small key/value settings table.
pub mod settings {
    use super::*;
    use rusqlite::{OptionalExtension, params};

    pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, key: &str) -> Result<()> {
        conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn secrets_round_trip() {
        let store = Store::memory().unwrap();
        let key = Arc::new(common::secrets::SecretKey::generate());
        let k = key.clone();
        store
            .write(move |c| secrets::set(c, &k, "smtp", "hunter2"))
            .await
            .unwrap();
        let k = key.clone();
        let v = store
            .read(move |c| secrets::get(c, &k, "smtp"))
            .await
            .unwrap();
        assert_eq!(v.unwrap().as_str(), "hunter2");
        let raw: String = store
            .read(|c| Ok(c.query_row("SELECT value FROM secrets", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert!(!raw.contains("hunter2"));
    }
}
