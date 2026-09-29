//! FR-G8 — SMS to guardians, through a local outbox.
//!
//! Phantom School Manager is offline-first, so a message is never sent in the moment it is
//! written. It is written to `sms_outbox` first, in the same transaction as
//! whatever caused it (a pass-out, a return), and a background sender drains
//! the outbox whenever the school has internet. A message written with no
//! connection simply waits; nothing is lost and nobody has to retry by hand.
//!
//! Two Ugandan gateways are supported: Africa's Talking and EgoSMS. The school
//! picks one in Settings and pays the gateway directly.

use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How often the background sender looks for queued messages.
const INTERVAL: Duration = Duration::from_secs(120);
/// A message that has failed this many times on the network stops retrying.
const MAX_ATTEMPTS: i64 = 8;
const TIMEOUT: Duration = Duration::from_secs(20);

pub const ID_PREFIX: &str = "sms";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsSettings {
    /// `off`, `africastalking` or `egosms`.
    pub provider: String,
    /// Africa's Talking username (use `sandbox` to test), or EgoSMS username.
    pub username: String,
    /// Africa's Talking API key, or EgoSMS password. Never sent back to the
    /// interface in full: `get_sms_settings` returns it masked.
    pub api_key: String,
    /// Registered sender ID. Blank uses the gateway's default.
    pub sender_id: String,
    /// How messages begin, e.g. "RAINBOW N&P". Blank uses the school name.
    pub signature: String,
}

pub fn load_settings(conn: &Connection) -> AppResult<SmsSettings> {
    let get = |key: &str| -> AppResult<String> {
        Ok(repo::get_setting(conn, key)?.unwrap_or_default())
    };
    let provider = get("sms.provider")?;
    Ok(SmsSettings {
        provider: if provider.is_empty() { "off".into() } else { provider },
        username: get("sms.username")?,
        api_key: get("sms.api_key")?,
        sender_id: get("sms.sender_id")?,
        signature: get("sms.signature")?,
    })
}

pub fn save_settings(conn: &Connection, settings: &SmsSettings) -> AppResult<()> {
    if !matches!(settings.provider.as_str(), "off" | "africastalking" | "egosms") {
        return Err(AppError::validation("Unknown SMS provider."));
    }
    repo::set_setting(conn, "sms.provider", &settings.provider)?;
    repo::set_setting(conn, "sms.username", settings.username.trim())?;
    repo::set_setting(conn, "sms.api_key", settings.api_key.trim())?;
    repo::set_setting(conn, "sms.sender_id", settings.sender_id.trim())?;
    repo::set_setting(conn, "sms.signature", settings.signature.trim())?;
    Ok(())
}

/// The words every message starts with.
pub fn signature(conn: &Connection) -> AppResult<String> {
    let custom = repo::get_setting(conn, "sms.signature")?.unwrap_or_default();
    if !custom.trim().is_empty() {
        return Ok(custom.trim().to_string());
    }
    Ok(conn.query_row("SELECT name FROM institution WHERE id = 1", [], |row| row.get(0))?)
}

// ---------------------------------------------------------------------------
// Phone numbers
// ---------------------------------------------------------------------------

/// A Ugandan number in international form, `+256XXXXXXXXX`, or `None` when
/// the text cannot be a phone number. Accepts the ways schools write them:
/// `0772 123456`, `772123456`, `256772123456`, `+256 772 123 456`.
pub fn normalize_phone(raw: &str) -> Option<String> {
    let plus = raw.trim_start().starts_with('+');
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();

    let national = if let Some(rest) = digits.strip_prefix("256") {
        rest.to_string()
    } else if plus {
        // Another country's number, already international.
        return (digits.len() >= 8 && digits.len() <= 15).then(|| format!("+{digits}"));
    } else if let Some(rest) = digits.strip_prefix('0') {
        rest.to_string()
    } else {
        digits
    };

    // Only mobile numbers (07XX) can receive an SMS.
    (national.len() == 9 && national.starts_with('7')).then(|| format!("+256{national}"))
}

/// How many SMS a message costs: 160 characters for one, 153 per part after.
pub fn segments(body: &str) -> usize {
    let len = body.chars().count();
    if len <= 160 {
        1
    } else {
        len.div_ceil(153)
    }
}

// ---------------------------------------------------------------------------
// The outbox
// ---------------------------------------------------------------------------

/// Writes a message to the outbox. Call it inside the same transaction as the
/// change that caused it. Returns the outbox id, or `None` (and queues
/// nothing) when the phone number is missing or unusable.
pub fn queue(
    conn: &Connection,
    phone: Option<&str>,
    body: &str,
    kind: &str,
    related_id: &str,
    created_by: Option<&str>,
) -> AppResult<Option<String>> {
    let Some(to) = phone.and_then(normalize_phone) else {
        return Ok(None);
    };
    let id = new_id(ID_PREFIX);
    conn.execute(
        "INSERT INTO sms_outbox (id, to_phone, body, kind, related_id, status, attempts, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, ?6, ?7)",
        params![id, to, body, kind, related_id, created_by, Utc::now().to_rfc3339()],
    )?;
    Ok(Some(id))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushResult {
    pub sent: usize,
    pub failed: usize,
    pub still_queued: usize,
    /// Why nothing was attempted, e.g. SMS is switched off.
    pub skipped_reason: Option<String>,
}

/// Sends everything queued. The database lock is held only to read the queue
/// and to record each result, never across a network call.
pub fn flush(state: &AppState) -> AppResult<FlushResult> {
    let (settings, batch) = {
        let conn = state.db.lock();
        let settings = load_settings(&conn)?;
        let mut stmt = conn.prepare(
            "SELECT id, to_phone, body FROM sms_outbox
             WHERE status = 'queued' AND attempts < ?1
             ORDER BY created_at ASC LIMIT 50",
        )?;
        let batch: Vec<(String, String, String)> = stmt
            .query_map(params![MAX_ATTEMPTS], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        (settings, batch)
    };

    let mut result = FlushResult {
        sent: 0,
        failed: 0,
        still_queued: 0,
        skipped_reason: None,
    };

    if batch.is_empty() {
        return Ok(result);
    }
    if settings.provider == "off" || settings.username.is_empty() || settings.api_key.is_empty() {
        result.still_queued = batch.len();
        result.skipped_reason =
            Some("SMS is not set up yet. Messages are waiting in the outbox.".into());
        return Ok(result);
    }

    for (id, to, body) in batch {
        let outcome = send(&settings, &to, &body);
        let conn = state.db.lock();
        let now = Utc::now().to_rfc3339();
        match outcome {
            Ok(reference) => {
                conn.execute(
                    "UPDATE sms_outbox SET status = 'sent', attempts = attempts + 1,
                            provider_ref = ?1, last_error = NULL, sent_at = ?2
                     WHERE id = ?3",
                    params![reference, now, id],
                )?;
                result.sent += 1;
            }
            Err(SendError::Offline(reason)) => {
                // No internet, or the gateway is unreachable: try again later.
                conn.execute(
                    "UPDATE sms_outbox SET attempts = attempts + 1, last_error = ?1 WHERE id = ?2",
                    params![reason, id],
                )?;
                result.still_queued += 1;
            }
            Err(SendError::Rejected(reason)) => {
                conn.execute(
                    "UPDATE sms_outbox SET status = 'failed', attempts = attempts + 1, last_error = ?1
                     WHERE id = ?2",
                    params![reason, id],
                )?;
                result.failed += 1;
            }
        }
    }

    Ok(result)
}

/// Sends queued messages on a background thread, so a screen never waits on
/// the network.
pub fn flush_in_background(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("rm-sms-now".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            if let Err(err) = flush(&state) {
                log::error!("sending queued SMS failed: {err}");
            }
        });
    if let Err(err) = spawned {
        log::error!("could not start the SMS sender: {err}");
    }
}

/// Retries the outbox every couple of minutes for as long as Phantom School Manager is open.
pub fn spawn_scheduler(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("rm-sms".into())
        .spawn(move || loop {
            std::thread::sleep(INTERVAL);
            let state = app.state::<AppState>();
            if let Err(err) = flush(&state) {
                log::error!("scheduled SMS send failed: {err}");
            }
        });
    if let Err(err) = spawned {
        log::error!("could not start the SMS scheduler: {err}");
    }
}

// ---------------------------------------------------------------------------
// Gateways
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SendError {
    /// Worth retrying: no connection, a timeout, or a gateway outage.
    Offline(String),
    /// Not worth retrying: bad credentials, a bad number, no credit.
    Rejected(String),
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .try_proxy_from_env(true)
        .build()
}

fn classify(err: ureq::Error) -> SendError {
    match err {
        ureq::Error::Status(code, response) => {
            let text = response.into_string().unwrap_or_default();
            let text = text.trim().chars().take(200).collect::<String>();
            if code >= 500 {
                SendError::Offline(format!("The SMS gateway had a problem ({code}). {text}"))
            } else {
                SendError::Rejected(format!("The SMS gateway refused the message ({code}). {text}"))
            }
        }
        ureq::Error::Transport(transport) => {
            SendError::Offline(format!("No connection to the SMS gateway: {transport}"))
        }
    }
}

/// Sends one message and returns the gateway's reference for it.
pub fn send(settings: &SmsSettings, to: &str, body: &str) -> Result<String, SendError> {
    match settings.provider.as_str() {
        "africastalking" => send_africastalking(settings, to, body),
        "egosms" => send_egosms(settings, to, body),
        _ => Err(SendError::Rejected("SMS is switched off.".into())),
    }
}

fn send_africastalking(settings: &SmsSettings, to: &str, body: &str) -> Result<String, SendError> {
    let url = if settings.username == "sandbox" {
        "https://api.sandbox.africastalking.com/version1/messaging"
    } else {
        "https://api.africastalking.com/version1/messaging"
    };

    let mut form: Vec<(&str, &str)> = vec![
        ("username", settings.username.as_str()),
        ("to", to),
        ("message", body),
    ];
    if !settings.sender_id.is_empty() {
        form.push(("from", settings.sender_id.as_str()));
    }

    let response = agent()
        .post(url)
        .set("apiKey", &settings.api_key)
        .set("Accept", "application/json")
        .send_form(&form)
        .map_err(classify)?;

    let text = response
        .into_string()
        .map_err(|err| SendError::Offline(err.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| SendError::Rejected(format!("Unexpected reply from Africa's Talking: {text}")))?;

    let recipient = &json["SMSMessageData"]["Recipients"][0];
    let code = recipient["statusCode"].as_i64().unwrap_or(0);
    // 100 processed, 101 sent, 102 queued by the gateway.
    if (100..=102).contains(&code) {
        Ok(recipient["messageId"].as_str().unwrap_or("").to_string())
    } else {
        let status = recipient["status"]
            .as_str()
            .or_else(|| json["SMSMessageData"]["Message"].as_str())
            .unwrap_or("Unknown error");
        Err(SendError::Rejected(format!("Africa's Talking: {status}")))
    }
}

fn send_egosms(settings: &SmsSettings, to: &str, body: &str) -> Result<String, SendError> {
    // EgoSMS takes the number without the plus sign.
    let number = to.trim_start_matches('+');
    let sender = if settings.sender_id.is_empty() {
        "EgoSMS"
    } else {
        settings.sender_id.as_str()
    };

    let response = agent()
        .get("https://www.egosms.co/api/v1/plain/")
        .query("number", number)
        .query("message", body)
        .query("username", &settings.username)
        .query("password", &settings.api_key)
        .query("sender", sender)
        .query("priority", "0")
        .call()
        .map_err(classify)?;

    let text = response
        .into_string()
        .map_err(|err| SendError::Offline(err.to_string()))?;
    if text.trim().eq_ignore_ascii_case("ok") {
        Ok(String::new())
    } else {
        Err(SendError::Rejected(format!("EgoSMS: {}", text.trim())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ugandan_numbers_in_every_common_shape() {
        for raw in [
            "0772 123456",
            "0772123456",
            "772123456",
            "256772123456",
            "+256 772 123 456",
            "+256-772-123-456",
        ] {
            assert_eq!(normalize_phone(raw).as_deref(), Some("+256772123456"), "{raw}");
        }
        // A landline cannot receive an SMS.
        assert_eq!(normalize_phone("0414 555010"), None);
    }

    #[test]
    fn nonsense_is_not_a_phone_number() {
        assert_eq!(normalize_phone(""), None);
        assert_eq!(normalize_phone("n/a"), None);
        assert_eq!(normalize_phone("0772 12"), None);
    }

    #[test]
    fn segments_follow_the_gsm_rule() {
        assert_eq!(segments(&"a".repeat(160)), 1);
        assert_eq!(segments(&"a".repeat(161)), 2);
        assert_eq!(segments(&"a".repeat(306)), 2);
        assert_eq!(segments(&"a".repeat(307)), 3);
    }
}
