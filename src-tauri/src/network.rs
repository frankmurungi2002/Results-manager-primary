//! Whether this school lets Termbook use the internet at all.
//!
//! Everything a school does every day (marks, report cards, fees, attendance,
//! printing) works with no internet and always will. Only four things ever
//! reach the network, and each is optional:
//!
//! * sending SMS through the school's gateway,
//! * checking the SMS credit balance,
//! * checking for and downloading updates,
//! * email invitations (through Termbook Cloud, once it exists).
//!
//! A school with no data bundle, or one that simply does not want a computer
//! that talks to the internet, switches on **Offline only**. Then none of the
//! four happens: nothing is attempted, nothing waits to be sent, and the
//! screens that need the internet say so instead of failing. This module is
//! the one place that question is answered, so no feature can forget to ask.

use rusqlite::Connection;
use serde::Serialize;

use crate::domain::repo;
use crate::error::{AppError, AppResult};

const KEY: &str = "network.offline_only";

/// Shown wherever something needs the internet and the school said no.
pub const OFFLINE_ONLY_MESSAGE: &str =
    "This school uses Termbook offline only. To use this, turn off \"Offline only\" in Settings → Internet.";

pub fn offline_only(conn: &Connection) -> AppResult<bool> {
    Ok(repo::get_setting(conn, KEY)?.as_deref() == Some("1"))
}

pub fn set_offline_only(conn: &Connection, on: bool) -> AppResult<()> {
    repo::set_setting(conn, KEY, if on { "1" } else { "0" })
}

/// Fails with a plain explanation when the school chose offline only.
pub fn require_online_allowed(conn: &Connection) -> AppResult<()> {
    if offline_only(conn)? {
        Err(AppError::validation(OFFLINE_ONLY_MESSAGE))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connectivity {
    /// The school switched every internet feature off.
    pub offline_only: bool,
    /// Messages waiting in the outbox, which offline only will never send.
    pub queued_messages: i64,
}

pub fn connectivity(conn: &Connection) -> AppResult<Connectivity> {
    Ok(Connectivity {
        offline_only: offline_only(conn)?,
        queued_messages: conn.query_row(
            "SELECT COUNT(*) FROM sms_outbox WHERE status = 'queued'",
            [],
            |row| row.get(0),
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn online_by_default_and_the_switch_sticks() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.lock();
        assert!(!offline_only(&conn).unwrap(), "a new school is not offline only");
        assert!(require_online_allowed(&conn).is_ok());

        set_offline_only(&conn, true).unwrap();
        assert!(offline_only(&conn).unwrap());
        let refused = require_online_allowed(&conn).unwrap_err();
        assert!(refused.user_message().contains("offline only"));

        set_offline_only(&conn, false).unwrap();
        assert!(!offline_only(&conn).unwrap());
    }
}
