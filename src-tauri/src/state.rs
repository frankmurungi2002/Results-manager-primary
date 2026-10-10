//! Process-wide state: the database handle, the session, and where files live.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::security::SessionStore;

pub struct AppState {
    pub db: Database,
    pub sessions: SessionStore,
    pub paths: AppPaths,
    /// The database's change counter when the last snapshot was taken; see
    /// `backup::take_if_changed`.
    backed_up_at_change: AtomicU64,
}

impl AppState {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        let db = Database::open(&paths.db_file)?;
        // A lapsed subscription is read-only from the first statement.
        crate::licence::refresh(&db.lock())?;
        let changes = db.total_changes();
        Ok(Self {
            db,
            sessions: SessionStore::new(),
            paths,
            backed_up_at_change: AtomicU64::new(changes),
        })
    }

    pub fn mark_backed_up(&self, total_changes: u64) {
        self.backed_up_at_change.store(total_changes, Ordering::SeqCst);
    }

    pub fn has_unbacked_changes(&self, total_changes: u64) -> bool {
        total_changes != self.backed_up_at_change.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Everything Termbook owns on this machine.
    pub data_dir: PathBuf,
    /// The live database.
    pub db_file: PathBuf,
    /// Local scheduled backups (simplified FR-B12, per SRS 16.1).
    pub backup_dir: PathBuf,
    /// Where exports land before the user picks a destination.
    pub export_dir: PathBuf,
}

/// The data folder's name before the product became Termbook. The folder is
/// named after the app's identifier, so the rename gave it a new one.
const OLD_DATA_FOLDER: &str = "ug.resultsmanager.desktop";

/// Brings a school's data across from the folder used before the rename, the
/// first time Termbook starts. Moves the whole folder (database, backups,
/// exports) when Termbook has no database of its own yet; never overwrites
/// one. Returns whether anything was moved.
pub fn adopt_previous_folder(data_dir: &Path) -> std::io::Result<bool> {
    let Some(parent) = data_dir.parent() else { return Ok(false) };
    let old = parent.join(OLD_DATA_FOLDER);
    if data_dir.join("school.rmdb").exists() || !old.join("school.rmdb").exists() {
        return Ok(false);
    }
    if !data_dir.exists() {
        std::fs::rename(&old, data_dir)?;
        return Ok(true);
    }
    // The new folder exists but holds no database: move the contents in.
    for entry in std::fs::read_dir(&old)? {
        let entry = entry?;
        let target = data_dir.join(entry.file_name());
        if !target.exists() {
            std::fs::rename(entry.path(), target)?;
        }
    }
    Ok(true)
}

impl AppPaths {
    pub fn resolve(app_data_dir: PathBuf) -> AppResult<Self> {
        let data_dir = app_data_dir;
        let paths = AppPaths {
            db_file: data_dir.join("school.rmdb"),
            backup_dir: data_dir.join("backups"),
            export_dir: data_dir.join("exports"),
            data_dir,
        };

        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| AppError::internal(format!("could not create the data folder: {e}")))?;
        std::fs::create_dir_all(&paths.backup_dir)
            .map_err(|e| AppError::internal(format!("could not create the backup folder: {e}")))?;
        std::fs::create_dir_all(&paths.export_dir)
            .map_err(|e| AppError::internal(format!("could not create the export folder: {e}")))?;

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_from_before_the_rename_is_carried_across_once() {
        let root = std::env::temp_dir().join(format!("tb-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join(OLD_DATA_FOLDER);
        std::fs::create_dir_all(old.join("backups")).unwrap();
        std::fs::write(old.join("school.rmdb"), b"school").unwrap();
        std::fs::write(old.join("backups").join("a.rmdb"), b"snap").unwrap();
        let new = root.join("ug.co.termbook");

        assert!(adopt_previous_folder(&new).unwrap());
        assert_eq!(std::fs::read(new.join("school.rmdb")).unwrap(), b"school");
        assert!(new.join("backups").join("a.rmdb").exists());
        assert!(!adopt_previous_folder(&new).unwrap(), "only once");

        // Never over a database Termbook already has.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("school.rmdb"), b"older").unwrap();
        assert!(!adopt_previous_folder(&new).unwrap());
        assert_eq!(std::fs::read(new.join("school.rmdb")).unwrap(), b"school");
        let _ = std::fs::remove_dir_all(&root);
    }
}
