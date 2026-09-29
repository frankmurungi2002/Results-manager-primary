//! Database handle, connection pragmas, and key management.
//!
//! There is exactly one connection, guarded by a mutex. A school runs Phantom School Manager on one
//! PC with at most a handful of concurrent operations, so a pool buys nothing
//! and a single connection makes transaction boundaries obvious.

pub mod migrations;

use std::path::{Path, PathBuf};

use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;

use crate::error::AppResult;

pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// Opens (creating if needed) the school database at `path` and brings the
    /// schema up to date.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut conn = Connection::open(path)?;

        #[cfg(feature = "encrypted-db")]
        apply_encryption_key(&conn, path)?;

        apply_pragmas(&conn)?;
        migrations::run(&mut conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// In-memory database, for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        apply_pragmas(&conn)?;
        migrations::run(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Snapshot the live database to `dest` using SQLite's online backup API,
    /// which is safe to run while the app is in use.
    ///
    /// This is the simplified backup mechanism SRS 16.1 recommends for the
    /// pilot in place of a hand-built transactional mirror: a scheduled
    /// copy-to-second-drive that still covers the failure that actually
    /// happens — one drive dying.
    pub fn backup_to(&self, dest: &Path) -> AppResult<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = self.lock();
        let mut target = Connection::open(dest)?;

        #[cfg(feature = "encrypted-db")]
        apply_encryption_key(&target, dest)?;

        let backup = rusqlite::backup::Backup::new(&conn, &mut target)?;
        backup.run_to_completion(256, std::time::Duration::from_millis(50), None)?;
        Ok(())
    }

    /// Changes written through the live connection since it was opened. The
    /// backup scheduler compares this with its last snapshot to skip copying
    /// a database nobody has touched.
    pub fn total_changes(&self) -> u64 {
        self.lock().total_changes()
    }

    /// Replaces the live database's contents with the snapshot at `src`, in
    /// place, then brings its schema up to date. The connection stays open, so
    /// nothing else in the process has to reconnect.
    ///
    /// The caller is responsible for checking the snapshot first
    /// (`open_snapshot`) and for keeping a copy of what is being replaced.
    pub fn restore_from(&self, src: &Path) -> AppResult<()> {
        let source = open_snapshot(src)?;
        let mut conn = self.lock();
        {
            let restore = rusqlite::backup::Backup::new(&source, &mut conn)?;
            restore.run_to_completion(256, std::time::Duration::from_millis(50), None)?;
        }
        apply_pragmas(&conn)?;
        migrations::run(&mut conn)?;
        Ok(())
    }
}

/// Opens a backup file read-only, for inspecting or restoring it. Never used
/// for the live database.
pub fn open_snapshot(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    #[cfg(feature = "encrypted-db")]
    apply_encryption_key(&conn, path)?;

    Ok(conn)
}

/// Connection settings applied on every open.
///
/// `execute_batch` rather than `pragma_update`, because several of these
/// (notably `journal_mode`) return a row, and `pragma_update` treats a
/// row-returning statement as an error.
fn apply_pragmas(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        -- Referential integrity is off by default in SQLite. Turn it on: the
        -- never-delete rules in the schema depend on it.
        PRAGMA foreign_keys = ON;

        -- WAL survives an abrupt power cut far better than the rollback
        -- journal, which matters on Ugandan mains (SRS 13, power-loss).
        PRAGMA journal_mode = WAL;

        -- FULL, not NORMAL: a school loses power mid-term-end and we would
        -- rather spend the fsync than explain a missing mark.
        PRAGMA synchronous = FULL;

        PRAGMA busy_timeout = 5000;
        PRAGMA temp_store = MEMORY;

        -- 64 MiB page cache. Comfortable on the 8 GB minimum spec and enough
        -- to keep an 800-learner class list responsive (SRS 14.6).
        PRAGMA cache_size = -64000;
        ",
    )?;

    Ok(())
}

/// Unlocks a SQLCipher database, creating and storing the key on first run.
///
/// The key lives in the Windows Credential Manager (or the platform keychain),
/// never on the SSDs alongside the data, so a stolen drive is not a readable
/// drive.
#[cfg(feature = "encrypted-db")]
fn apply_encryption_key(conn: &Connection, path: &Path) -> AppResult<()> {
    use crate::error::AppError;
    use rand::RngCore;

    const SERVICE: &str = "ug.resultsmanager.desktop";
    let account = format!("db::{}", path.display());

    let entry = keyring::Entry::new(SERVICE, &account)
        .map_err(|e| AppError::internal(format!("keychain unavailable: {e}")))?;

    let key_hex = match entry.get_password() {
        Ok(existing) => existing,
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
            entry
                .set_password(&hex)
                .map_err(|e| AppError::internal(format!("could not store database key: {e}")))?;
            hex
        }
        Err(e) => {
            return Err(AppError::internal(format!(
                "could not read the database key: {e}"
            )))
        }
    };

    // The x'..' form hands SQLCipher raw key bytes, skipping passphrase key
    // derivation — correct when the key is already 256 bits of entropy.
    // `key_hex` is our own hex of random bytes, so there is nothing to inject.
    conn.execute_batch(&format!(
        "PRAGMA key = \"x'{key_hex}'\"; PRAGMA cipher_memory_security = ON;"
    ))?;

    // Prove the key actually opened the file before going any further.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| {
            AppError::internal("the database could not be unlocked with the stored key".to_string())
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_cleanly() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .expect("version");
        assert_eq!(version, migrations::MIGRATIONS.last().unwrap().version);
    }

    #[test]
    fn audit_log_rejects_updates() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        conn.execute(
            "INSERT INTO audit_log (at, action, summary) VALUES ('now', 'test', 'x')",
            [],
        )
        .expect("insert");

        let update = conn.execute("UPDATE audit_log SET summary = 'y'", []);
        assert!(update.is_err(), "audit_log must reject UPDATE");

        let delete = conn.execute("DELETE FROM audit_log", []);
        assert!(delete.is_err(), "audit_log must reject DELETE");
    }

    #[test]
    fn uneb_bands_cover_every_percentage() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        for pct in 0..=100 {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM grading_bands
                     WHERE grading_system_id = 'gs_uneb_primary'
                       AND ?1 BETWEEN lower_bound AND upper_bound",
                    [pct],
                    |r| r.get(0),
                )
                .expect("query");
            assert_eq!(count, 1, "percentage {pct} must land in exactly one band");
        }
    }
}
