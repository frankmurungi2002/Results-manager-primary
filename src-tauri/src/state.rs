//! Process-wide state: the database handle, the session, and where files live.

use std::path::PathBuf;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::security::SessionStore;

pub struct AppState {
    pub db: Database,
    pub sessions: SessionStore,
    pub paths: AppPaths,
}

impl AppState {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        let db = Database::open(&paths.db_file)?;
        Ok(Self {
            db,
            sessions: SessionStore::new(),
            paths,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Everything RM owns on this machine.
    pub data_dir: PathBuf,
    /// The live database.
    pub db_file: PathBuf,
    /// Local scheduled backups (simplified FR-B12, per SRS 16.1).
    pub backup_dir: PathBuf,
    /// Where exports land before the user picks a destination.
    pub export_dir: PathBuf,
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
