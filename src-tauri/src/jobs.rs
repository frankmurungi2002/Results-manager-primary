//! Things Termbook does by itself while it is open: the owner's evening
//! summary, fee and absence reminders. Each job is a plain function of the
//! database and the local time, so it is tested without waiting for a clock;
//! this module only calls them every few minutes.
//!
//! Every job is off until the school switches it on, and none of them sends
//! anything directly: they write to the SMS outbox like everything else, so
//! offline only, priorities and the one-sender rule all still apply.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(5 * 60);

/// Runs every job once, as of `now` (the school's local time).
pub fn run_all(state: &AppState, now: chrono::NaiveDateTime) {
    let conn = state.db.lock();
    let mut queued = 0;
    for (name, job) in JOBS {
        match job(&conn, now) {
            Ok(n) => queued += n,
            Err(err) => log::error!("background job {name} failed: {err}"),
        }
    }
    drop(conn);
    if queued > 0 {
        crate::sms::wake();
    }
}

type Job = fn(&rusqlite::Connection, chrono::NaiveDateTime) -> crate::error::AppResult<usize>;

const JOBS: &[(&str, Job)] = &[
    ("owner summary", crate::commands::daybook::owner_summary_job),
    ("fee reminders", crate::commands::autosms::fee_reminder_job),
    ("promise reminders", crate::commands::autosms::promise_reminder_job),
    ("not reported", crate::commands::autosms::not_reported_job),
    ("marks reminders", crate::commands::deadlines::marks_reminder_job),
];

pub fn spawn_scheduler(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("rm-jobs".into()).spawn(move || loop {
        std::thread::sleep(TICK);
        let state = app.state::<AppState>();
        run_all(&state, chrono::Local::now().naive_local());
    });
    if let Err(err) = spawned {
        log::error!("could not start the background jobs: {err}");
    }
}
