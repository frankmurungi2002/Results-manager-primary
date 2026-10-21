//! Updates: a fix released once reaches every school by itself.
//!
//! Releases are signed with the owner's updater key (Ed25519, minisign) and
//! published on the project's GitHub releases. The app checks once a day,
//! only when the school allows the internet, and installs only when someone
//! with the School Admin's rights presses Install: a school is never
//! restarted in the middle of entering marks.
//!
//! The public key is compiled in from `src-tauri/updater.pub`. Until the
//! owner puts one there (see docs/UPDATES.md), updates are switched off and
//! the screen says so; nothing is ever installed unsigned.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_updater::UpdaterExt;

use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::Permission;
use crate::state::AppState;

const PUBLIC_KEY: &str = include_str!("../updater.pub");
const ENDPOINT: &str = "https://github.com/frankmurungi2002/termbook-releases/releases/latest/download/latest.json";

pub fn configured() -> bool {
    !PUBLIC_KEY.trim().is_empty()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    /// The owner has set up signed updates for this build.
    pub configured: bool,
    pub offline_only: bool,
    pub last_checked: Option<String>,
    /// A newer version waiting to be installed.
    pub available_version: Option<String>,
    pub available_notes: Option<String>,
}

fn info(conn: &rusqlite::Connection) -> AppResult<UpdateInfo> {
    let available = repo::get_setting(conn, "update.available")?.filter(|v| !v.is_empty() && v != env!("CARGO_PKG_VERSION"));
    Ok(UpdateInfo {
        current_version: env!("CARGO_PKG_VERSION").into(),
        configured: configured(),
        offline_only: crate::network::offline_only(conn)?,
        last_checked: repo::get_setting(conn, "update.checked_at")?,
        available_notes: available.as_ref().and_then(|_| repo::get_setting(conn, "update.notes").ok().flatten()),
        available_version: available,
    })
}

#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> AppResult<UpdateInfo> {
    state.sessions.require()?;
    info(&state.db.lock())
}

fn updater<R: Runtime>(app: &AppHandle<R>) -> AppResult<tauri_plugin_updater::Updater> {
    if !configured() {
        return Err(AppError::validation("Updates are not set up in this copy of Termbook yet."));
    }
    let url = ENDPOINT.parse().map_err(|_| AppError::internal("bad update address"))?;
    app.updater_builder()
        .pubkey(PUBLIC_KEY.trim())
        .endpoints(vec![url])
        .and_then(|b| b.build())
        .map_err(|e| AppError::internal(format!("could not prepare the updater: {e}")))
}

/// Asks GitHub whether there is a newer version. Never holds the database
/// lock while the network is waited on.
async fn check<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<UpdateInfo> {
    crate::network::require_online_allowed(&state.db.lock())?;
    let found = updater(app)?
        .check()
        .await
        .map_err(|e| AppError::validation(format!("Could not reach the update server: {e}")))?;
    let conn = state.db.lock();
    repo::set_setting(&conn, "update.checked_at", &chrono::Utc::now().to_rfc3339())?;
    match &found {
        Some(update) => {
            repo::set_setting(&conn, "update.available", &update.version)?;
            repo::set_setting(&conn, "update.notes", update.body.as_deref().unwrap_or(""))?;
        }
        None => repo::set_setting(&conn, "update.available", "")?,
    }
    info(&conn)
}

#[tauri::command]
pub async fn check_for_update<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> AppResult<UpdateInfo> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    check(&app, &state).await
}

/// Downloads, checks the signature, installs and restarts. A backup is taken
/// first, so a bad release can always be undone.
#[tauri::command]
pub async fn install_update<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    crate::network::require_online_allowed(&state.db.lock())?;
    let Some(update) = updater(&app)?
        .check()
        .await
        .map_err(|e| AppError::validation(format!("Could not reach the update server: {e}")))?
    else {
        return Err(AppError::validation("Termbook is already up to date."));
    };
    crate::backup::take(&state, None, crate::backup::Trigger::Manual)?;
    {
        let conn = state.db.lock();
        crate::audit::record(
            &conn,
            Some(&session),
            "update.install",
            "app",
            &update.version,
            format!("Installing Termbook {} (was {})", update.version, env!("CARGO_PKG_VERSION")),
        )?;
    }
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| AppError::validation(format!("The update could not be installed: {e}")))?;
    app.restart();
}

/// Once a day, quietly, so the dashboard can say an update is waiting.
pub fn spawn_daily_check(app: AppHandle) {
    if !configured() {
        return;
    }
    let spawned = std::thread::Builder::new().name("rm-updates".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10 * 60));
        let state = app.state::<AppState>();
        let due = {
            let conn = state.db.lock();
            let offline = crate::network::offline_only(&conn).unwrap_or(true);
            let last = repo::get_setting(&conn, "update.checked_at").ok().flatten();
            let stale = last
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
                .map_or(true, |t| chrono::Utc::now() - t.with_timezone(&chrono::Utc) > chrono::Duration::hours(24));
            !offline && stale
        };
        if due {
            if let Err(err) = tauri::async_runtime::block_on(check(&app, &state)) {
                log::info!("update check: {err}");
            }
        }
        std::thread::sleep(Duration::from_secs(50 * 60));
    });
    if let Err(err) = spawned {
        log::error!("could not start the update checker: {err}");
    }
}
