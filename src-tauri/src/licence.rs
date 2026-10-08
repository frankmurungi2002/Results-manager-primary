//! FR-B11 — the school's subscription, checked offline.
//!
//! A school pays for a term or a year. The owner then issues a licence key:
//! the school's code, the plan and an end date, signed with an Ed25519 private
//! key that never leaves the owner (see `tools/phantom-licence`). This module
//! holds only the matching public key, so it can check a key but never make
//! one. A key cannot be forged, cannot be edited (changing the date breaks the
//! signature), and works only for the school it was issued to.
//!
//! When a subscription ends there are seven days of grace, then the school
//! becomes **read-only**: everything can still be seen, printed and backed up,
//! nothing can be added or changed. A school is never locked out of its own
//! records. Read-only is enforced in SQLite itself (an authorizer on the
//! connection), so no command can forget to check it.
//!
//! What this is not: protection against someone who patches the program. It
//! keeps honest schools honest and makes a lapsed subscription obvious, which
//! is what a manual-activation business needs (SRS 16.4).

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use ed25519_dalek::VerifyingKey;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// The owner's public key, as 64 hex characters, printed by
/// `phantom-licence keygen`. While it is empty the application runs as a
/// pilot: no keys, no limits. Setting it turns subscriptions on.
pub const ISSUER_PUBLIC_KEY_HEX: &str = "";

/// Days of full use after a subscription ends.
pub const GRACE_DAYS: i64 = 7;
/// Days of full use after setup, before the first key is needed.
pub const TRIAL_DAYS: i64 = 30;

const SCHOOL_CODE: &str = "licence.school_code";
const KEY: &str = "licence.key";
const LATEST_SEEN: &str = "licence.latest_seen";

pub use crate::licence_format::{encode, Claims};

/// Reads a key and checks its signature. Says nothing yet about dates or
/// which school it is for.
pub fn decode(key: &str, issuer: &VerifyingKey) -> AppResult<Claims> {
    crate::licence_format::decode(key, issuer).map_err(AppError::validation)
}

fn issuer() -> Option<VerifyingKey> {
    if ISSUER_PUBLIC_KEY_HEX.is_empty() {
        return None;
    }
    crate::licence_format::public_key_from_hex(ISSUER_PUBLIC_KEY_HEX)
}

// ---------------------------------------------------------------------------
// The school's state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// No issuer key compiled in: the pilot, with no limits.
    Pilot,
    /// The first days after setup, before a key is needed.
    Trial,
    Active,
    /// Ended, but within the grace days: still fully usable.
    Grace,
    /// Past grace: read-only until a key is entered.
    Expired,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenceStatus {
    pub state: State,
    /// What the school sends the owner when it pays.
    pub school_code: String,
    pub read_only: bool,
    /// `term` or `year`, for an installed key.
    pub plan: Option<String>,
    pub name_on_key: Option<String>,
    pub key_id: Option<String>,
    /// The last day of full use: the key's end, the trial's end, or the end
    /// of grace.
    pub until: Option<NaiveDate>,
    /// Whole days until `until`; negative once past it.
    pub days_left: Option<i64>,
    /// The computer's clock was set earlier than a date already seen, so
    /// the later date is being used.
    pub clock_rolled_back: bool,
}

/// Today for licensing purposes: never earlier than a day already seen, so
/// turning the computer's clock back does not turn the subscription back on.
fn effective_today(conn: &Connection, now: DateTime<Utc>) -> AppResult<(NaiveDate, bool)> {
    let seen: Option<DateTime<Utc>> = crate::domain::repo::get_setting(conn, LATEST_SEEN)?
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc));
    let rolled_back = seen.is_some_and(|s| now < s - Duration::days(1));
    let latest = seen.map_or(now, |s| s.max(now));
    if seen.map_or(true, |s| now > s) {
        crate::domain::repo::set_setting(conn, LATEST_SEEN, &now.to_rfc3339())?;
    }
    Ok((latest.with_timezone(&Local).date_naive(), rolled_back))
}

pub fn school_code(conn: &Connection) -> AppResult<String> {
    if let Some(code) = crate::domain::repo::get_setting(conn, SCHOOL_CODE)? {
        return Ok(code);
    }
    // Set by migration 013; generated here only for a database that somehow
    // lost it.
    let code: String = conn.query_row("SELECT upper(hex(randomblob(5)))", [], |r| r.get(0))?;
    crate::domain::repo::set_setting(conn, SCHOOL_CODE, &code)?;
    Ok(code)
}

/// The school's state now, using the compiled-in issuer key.
pub fn status(conn: &Connection) -> AppResult<LicenceStatus> {
    status_with(conn, issuer().as_ref(), Utc::now())
}

pub fn status_with(conn: &Connection, issuer: Option<&VerifyingKey>, now: DateTime<Utc>) -> AppResult<LicenceStatus> {
    let school_code = school_code(conn)?;
    let (today, clock_rolled_back) = effective_today(conn, now)?;
    let mut status = LicenceStatus {
        state: State::Pilot,
        school_code: school_code.clone(),
        read_only: false,
        plan: None,
        name_on_key: None,
        key_id: None,
        until: None,
        days_left: None,
        clock_rolled_back,
    };
    let Some(issuer) = issuer else {
        return Ok(status);
    };

    let installed = crate::domain::repo::get_setting(conn, KEY)?
        .and_then(|key| decode(&key, issuer).ok())
        .filter(|claims| claims.school == school_code);

    let (state, until) = match installed {
        Some(claims) => {
            status.plan = Some(claims.plan.clone());
            status.name_on_key = Some(claims.name.clone());
            status.key_id = Some(claims.id.clone());
            if today <= claims.until {
                (State::Active, claims.until)
            } else if today <= claims.until + Duration::days(GRACE_DAYS) {
                (State::Grace, claims.until + Duration::days(GRACE_DAYS))
            } else {
                (State::Expired, claims.until + Duration::days(GRACE_DAYS))
            }
        }
        None => {
            let setup: Option<String> = conn
                .query_row("SELECT setup_completed_at FROM institution WHERE id = 1", [], |r| r.get(0))
                .optional()?
                .flatten();
            let started = setup
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Local).date_naive())
                .unwrap_or(today);
            let end = started + Duration::days(TRIAL_DAYS);
            (if today <= end { State::Trial } else { State::Expired }, end)
        }
    };
    status.state = state;
    status.read_only = state == State::Expired;
    status.until = Some(until);
    status.days_left = Some((until - today).num_days());
    Ok(status)
}

/// Checks a key against this school and stores it. Refuses a key for another
/// school, and one that would shorten a subscription already installed.
pub fn install(conn: &Connection, key: &str) -> AppResult<Claims> {
    let issuer = issuer().ok_or_else(|| {
        AppError::validation("This copy of Phantom does not use licence keys.")
    })?;
    install_with(conn, key, &issuer)
}

pub fn install_with(conn: &Connection, key: &str, issuer: &VerifyingKey) -> AppResult<Claims> {
    let claims = decode(key, issuer)?;
    let code = school_code(conn)?;
    if claims.school != code {
        return Err(AppError::validation(format!(
            "That key is for school code {}, but this school's code is {code}. Check you were sent the right key.",
            claims.school
        )));
    }
    if let Some(current) = crate::domain::repo::get_setting(conn, KEY)?.and_then(|k| decode(&k, issuer).ok()) {
        if current.school == code && current.until > claims.until {
            return Err(AppError::validation(format!(
                "The key already installed runs to {}, later than this one ({}). Keeping it.",
                current.until, claims.until
            )));
        }
    }
    crate::domain::repo::set_setting(conn, KEY, key.trim())?;
    Ok(claims)
}

// ---------------------------------------------------------------------------
// Read-only, enforced by SQLite
// ---------------------------------------------------------------------------

/// What a read-only school may still write, and why.
fn allowed_while_read_only(action: &rusqlite::hooks::AuthAction<'_>) -> bool {
    use rusqlite::hooks::AuthAction::*;
    match action {
        // The record of what happened keeps being written: who signed in,
        // who printed what. Its seal goes with it.
        Insert { table_name } => matches!(*table_name, "audit_log" | "audit_seal" | "settings" | "sqlite_sequence"),
        Update { table_name, .. } => matches!(
            *table_name,
            // Sign-in bookkeeping and password changes.
            "users"
            // Theme, last backup time, the clock high-water mark, a new key.
            | "settings"
            | "sqlite_sequence"
            // Texts queued before the lapse still go out, so their status
            // must update; otherwise each would be resent every round,
            // texting parents again and again and charging the school.
            | "sms_outbox"
            // A learner already out on a pass-out can still be marked back.
            | "pass_outs"
        ),
        Delete { .. } => false,
        CreateIndex { .. } | CreateTable { .. } | CreateTrigger { .. } | CreateView { .. }
        | DropIndex { .. } | DropTable { .. } | DropTrigger { .. } | DropView { .. }
        | AlterTable { .. } => false,
        _ => true,
    }
}

/// Turns read-only on or off for every statement prepared on `conn` from now
/// on.
pub fn apply(conn: &Connection, read_only: bool) {
    if read_only {
        conn.authorizer(Some(|ctx: rusqlite::hooks::AuthContext<'_>| {
            if allowed_while_read_only(&ctx.action) {
                rusqlite::hooks::Authorization::Allow
            } else {
                rusqlite::hooks::Authorization::Deny
            }
        }));
    } else {
        conn.authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>);
    }
}

/// Works out the state now and applies it to the connection.
pub fn refresh(conn: &Connection) -> AppResult<LicenceStatus> {
    let status = status(conn)?;
    apply(conn, status.read_only);
    Ok(status)
}

/// True when a database error is read-only mode refusing a write.
pub fn is_read_only_refusal(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::AuthorizationForStatementDenied)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};

    fn owner() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn claims(school: &str, from: &str, until: &str) -> Claims {
        Claims {
            v: 1,
            school: school.into(),
            name: "Rainbow N&P".into(),
            plan: "term".into(),
            from: from.parse().unwrap(),
            until: until.parse().unwrap(),
            id: "R-0001".into(),
        }
    }

    fn key(c: &Claims) -> String {
        let signer = owner();
        encode(c, |body| signer.sign(body).to_bytes())
    }

    fn at(day: &str) -> DateTime<Utc> {
        format!("{day}T09:00:00Z").parse().unwrap()
    }

    fn db_set_up_on(day: &str) -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute(
                "INSERT INTO institution (id, name, setup_completed_at, created_at, updated_at)
                 VALUES (1, 'Rainbow', ?1, 'now', 'now')",
                [format!("{day}T08:00:00+03:00")],
            )
            .unwrap();
        db
    }

    #[test]
    fn a_genuine_key_round_trips() {
        let c = claims("ABC123", "2026-09-01", "2026-12-12");
        assert_eq!(decode(&key(&c), &owner().verifying_key()).unwrap(), c);
    }

    #[test]
    fn changing_one_character_of_a_key_breaks_it() {
        let k = key(&claims("ABC123", "2026-09-01", "2026-12-12"));
        // Push the end date to next year by editing the encoded body.
        let parts: Vec<&str> = k.split('.').collect();
        let mut body = URL_SAFE_NO_PAD.decode(parts[1]).unwrap();
        let text = String::from_utf8(body.clone()).unwrap().replace("2026-12-12", "2027-12-12");
        body = text.into_bytes();
        let forged = format!("{}.{}.{}", parts[0], URL_SAFE_NO_PAD.encode(body), parts[2]);
        assert!(decode(&forged, &owner().verifying_key()).is_err());
    }

    #[test]
    fn a_key_signed_by_anyone_else_is_refused() {
        let impostor = SigningKey::from_bytes(&[9u8; 32]);
        let k = encode(&claims("ABC123", "2026-09-01", "2026-12-12"), |b| impostor.sign(b).to_bytes());
        assert!(decode(&k, &owner().verifying_key()).is_err());
    }

    #[test]
    fn a_key_for_another_school_is_refused() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        let err = install_with(&conn, &key(&claims("OTHER1", "2026-09-01", "2026-12-12")), &owner().verifying_key());
        assert!(err.unwrap_err().to_string().contains("school code"));
    }

    #[test]
    fn trial_then_expired_without_a_key() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        let issuer = owner().verifying_key();
        let s = status_with(&conn, Some(&issuer), at("2026-09-20")).unwrap();
        assert_eq!((s.state, s.read_only), (State::Trial, false));
        let s = status_with(&conn, Some(&issuer), at("2026-10-15")).unwrap();
        assert_eq!((s.state, s.read_only), (State::Expired, true));
    }

    #[test]
    fn a_term_key_runs_then_grace_then_read_only() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        let issuer = owner().verifying_key();
        let code = school_code(&conn).unwrap();
        install_with(&conn, &key(&claims(&code, "2026-09-01", "2026-12-12")), &issuer).unwrap();

        let s = status_with(&conn, Some(&issuer), at("2026-12-12")).unwrap();
        assert_eq!((s.state, s.days_left), (State::Active, Some(0)));
        let s = status_with(&conn, Some(&issuer), at("2026-12-15")).unwrap();
        assert_eq!((s.state, s.read_only), (State::Grace, false));
        let s = status_with(&conn, Some(&issuer), at("2026-12-20")).unwrap();
        assert_eq!((s.state, s.read_only), (State::Expired, true));
    }

    #[test]
    fn turning_the_clock_back_does_not_turn_the_subscription_back_on() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        let issuer = owner().verifying_key();
        let code = school_code(&conn).unwrap();
        install_with(&conn, &key(&claims(&code, "2026-09-01", "2026-12-12")), &issuer).unwrap();
        assert!(status_with(&conn, Some(&issuer), at("2027-01-05")).unwrap().read_only);
        // The office sets the computer back to November.
        let s = status_with(&conn, Some(&issuer), at("2026-11-01")).unwrap();
        assert!(s.read_only && s.clock_rolled_back);
    }

    #[test]
    fn an_older_key_cannot_replace_a_longer_one() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        let issuer = owner().verifying_key();
        let code = school_code(&conn).unwrap();
        install_with(&conn, &key(&claims(&code, "2026-09-01", "2027-08-31")), &issuer).unwrap();
        assert!(install_with(&conn, &key(&claims(&code, "2026-09-01", "2026-12-12")), &issuer).is_err());
    }

    #[test]
    fn without_an_issuer_key_the_pilot_has_no_limits() {
        let db = db_set_up_on("2020-01-01");
        let s = status_with(&db.lock(), None, at("2026-10-08")).unwrap();
        assert_eq!((s.state, s.read_only), (State::Pilot, false));
    }

    #[test]
    fn read_only_refuses_school_work_but_keeps_the_record_and_the_outbox() {
        let db = db_set_up_on("2026-09-01");
        let conn = db.lock();
        conn.execute(
            "INSERT INTO sms_outbox (id, to_phone, body, kind, created_at) VALUES ('m1', '+256772000001', 'x', 'bulk', 'now')",
            [],
        )
        .unwrap();
        apply(&conn, true);

        let refused = conn.execute(
            "INSERT INTO students (id, reg_number, full_name, created_at, updated_at) VALUES ('s', 'R', 'New', 'now', 'now')",
            [],
        );
        assert!(refused.as_ref().is_err_and(is_read_only_refusal), "{refused:?}");
        assert!(conn.execute("DELETE FROM settings", []).is_err());
        assert!(conn.execute("CREATE TABLE sneaky (x)", []).is_err());

        // Reading, the activity log and in-flight texts all still work.
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM classes", [], |r| r.get(0)).unwrap();
        assert!(n >= 0);
        crate::audit::record_system(&conn, "auth.sign_in", "user", "u", "Someone signed in").unwrap();
        conn.execute("UPDATE sms_outbox SET status = 'sent' WHERE id = 'm1'", []).unwrap();
        assert!(conn
            .execute("INSERT INTO sms_outbox (id, to_phone, body, kind, created_at) VALUES ('m2', '+256772000002', 'y', 'bulk', 'now')", [])
            .is_err(), "no new texts while read-only");

        apply(&conn, false);
        conn.execute(
            "INSERT INTO students (id, reg_number, full_name, created_at, updated_at) VALUES ('s', 'R', 'New', 'now', 'now')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn a_new_database_gets_a_school_code() {
        let db = Database::open_in_memory().unwrap();
        let code = school_code(&db.lock()).unwrap();
        assert_eq!(code.len(), 10);
        assert_eq!(code, school_code(&db.lock()).unwrap(), "stable");
    }
}
