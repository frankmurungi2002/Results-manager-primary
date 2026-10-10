//! FR-G8 — SMS to guardians, through a local outbox.
//!
//! Termbook is offline-first, so a message is never sent in the moment it is
//! written. It is written to `sms_outbox` first, in the same transaction as
//! whatever caused it (a pass-out, a return), and a background sender drains
//! the outbox whenever the school has internet. A message written with no
//! connection simply waits; nothing is lost and nobody has to retry by hand.
//!
//! Two Ugandan gateways are supported: Africa's Talking and EgoSMS. The school
//! picks one in Settings and pays the gateway directly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::db::Database;
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How often the background sender looks for queued messages.
const INTERVAL: Duration = Duration::from_secs(120);
const TIMEOUT: Duration = Duration::from_secs(20);

pub const ID_PREFIX: &str = "sms";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsSettings {
    /// `off`, `africastalking`, `egosms` (EgoSMS Comms, comms.egosms.co) or
    /// `egosms_classic` (accounts on the older www.egosms.co API).
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
    /// Whether Class Teachers and teachers in charge of a group may text
    /// their own parents (FR-G23). The office always may.
    #[serde(default = "yes")]
    pub teachers_may_broadcast: bool,
    /// What the gateway charges for one SMS, to say how many texts the
    /// credit buys. Shillings.
    #[serde(default = "default_price")]
    pub price_per_sms: f64,
    /// Warn when the credit falls below this.
    #[serde(default = "default_warn")]
    pub warn_below: f64,
}

fn default_price() -> f64 {
    35.0
}

fn default_warn() -> f64 {
    5_000.0
}

fn yes() -> bool {
    true
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
        teachers_may_broadcast: get("sms.teachers_may_broadcast")? != "0",
        price_per_sms: get("sms.price")?.parse().unwrap_or_else(|_| default_price()),
        warn_below: get("sms.balance_warn")?.parse().unwrap_or_else(|_| default_warn()),
    })
}

pub fn save_settings(conn: &Connection, settings: &SmsSettings) -> AppResult<()> {
    if !matches!(settings.provider.as_str(), "off" | "africastalking" | "egosms" | "egosms_classic") {
        return Err(AppError::validation("Unknown SMS provider."));
    }
    repo::set_setting(conn, "sms.provider", &settings.provider)?;
    repo::set_setting(conn, "sms.username", settings.username.trim())?;
    repo::set_setting(conn, "sms.api_key", settings.api_key.trim())?;
    repo::set_setting(conn, "sms.sender_id", settings.sender_id.trim())?;
    repo::set_setting(conn, "sms.signature", settings.signature.trim())?;
    repo::set_setting(
        conn,
        "sms.teachers_may_broadcast",
        if settings.teachers_may_broadcast { "1" } else { "0" },
    )?;
    if !(settings.price_per_sms >= 0.0 && settings.warn_below >= 0.0) {
        return Err(AppError::validation("The price and the warning level must be zero or more."));
    }
    repo::set_setting(conn, "sms.price", &settings.price_per_sms.to_string())?;
    repo::set_setting(conn, "sms.balance_warn", &settings.warn_below.to_string())?;
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

// ---------------------------------------------------------------------------
// What a message costs
// ---------------------------------------------------------------------------

/// The GSM 03.38 default alphabet: a message using only these characters
/// fits 160 to an SMS.
const GSM_BASIC: &str = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
/// Characters that exist in GSM but take two of the 160 places each.
const GSM_EXTENDED: &str = "^{}\\[~]|€\u{000C}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// 160 characters for one SMS, 153 per part after that.
    Gsm,
    /// Any character outside GSM makes the whole message Unicode:
    /// 70 for one SMS, 67 per part after. The same text can cost twice as much.
    Unicode,
}

/// How a message will be encoded, and how many SMS it costs to send once.
pub fn cost(body: &str) -> (Encoding, usize) {
    let mut septets = 0usize;
    for c in body.chars() {
        if GSM_BASIC.contains(c) {
            septets += 1;
        } else if GSM_EXTENDED.contains(c) {
            septets += 2;
        } else {
            let units = body.encode_utf16().count();
            let parts = if units <= 70 { 1 } else { units.div_ceil(67) };
            return (Encoding::Unicode, parts);
        }
    }
    let parts = if septets <= 160 { 1 } else { septets.div_ceil(153) };
    (Encoding::Gsm, parts)
}

/// How many SMS a message costs to send once.
pub fn segments(body: &str) -> usize {
    cost(body).1
}

/// Replaces the typographic characters word processors and phones insert
/// (curly quotes, long dashes, ellipses, non-breaking spaces) with their
/// plain equivalents. One curly apostrophe pasted from Word would otherwise
/// turn the whole message into Unicode and can double what the school pays.
pub fn plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{00B4}' | '`' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{00AB}' | '\u{00BB}' => out.push('"'),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{2009}' | '\u{200A}' | '\t' => out.push(' '),
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' => {}
            '\u{2022}' | '\u{00B7}' => out.push('-'),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The outbox
// ---------------------------------------------------------------------------

/// Urgent messages (pass-outs, absences) go before anything else.
pub const PRIORITY_URGENT: i64 = 0;
pub const PRIORITY_NORMAL: i64 = 1;
/// Bulk messages go last, so a pass-out never waits behind a broadcast.
pub const PRIORITY_BULK: i64 = 2;

/// How urgent each kind of message is, and how long it stays worth sending.
///
/// A pass-out or absence text is about today. Delivered the next morning it
/// says something that is no longer true, and alarms a parent for nothing, so
/// it expires twelve hours after it was written. Everything else waits for as
/// long as it takes.
pub fn policy(kind: &str) -> (i64, Option<chrono::Duration>) {
    match kind {
        "pass_out" | "pass_out_return" | "absence" => {
            (PRIORITY_URGENT, Some(chrono::Duration::hours(12)))
        }
        // A reminder that arrives days late is worse than none.
        "fee_reminder" | "promise_reminder" | "not_reported" | "daybook" => {
            (PRIORITY_BULK, Some(chrono::Duration::hours(24)))
        }
        "test" => (PRIORITY_URGENT, None),
        // An invite code is useless after three days.
        "invite" => (PRIORITY_URGENT, Some(chrono::Duration::hours(72))),
        "bulk" => (PRIORITY_BULK, None),
        _ => (PRIORITY_NORMAL, None),
    }
}

/// UTC to the second with a `Z`, the one format `expires_at` is compared in.
pub fn stamp(at: chrono::DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

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
    // A school that chose offline only gets the pass-out or register saved
    // without a text, never a text that waits for an internet it will not use.
    if crate::network::offline_only(conn)? {
        return Ok(None);
    }
    Ok(Some(insert(conn, &to, body, kind, Some(related_id), created_by)?))
}

/// Inserts one message for a number already normalised by [`normalize_phone`].
pub fn insert(
    conn: &Connection,
    to: &str,
    body: &str,
    kind: &str,
    related_id: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<String> {
    crate::network::require_online_allowed(conn)?;
    let (priority, lifetime) = policy(kind);
    let now = Utc::now();
    let id = new_id(ID_PREFIX);
    conn.execute(
        "INSERT INTO sms_outbox (id, to_phone, body, kind, related_id, status, attempts,
                                 priority, expires_at, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, ?6, ?7, ?8, ?9)",
        params![
            id,
            to,
            body,
            kind,
            related_id,
            priority,
            lifetime.map(|life| stamp(now + life)),
            created_by,
            now.to_rfc3339(),
        ],
    )?;
    Ok(id)
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushResult {
    pub sent: usize,
    pub failed: usize,
    pub still_queued: usize,
    /// Closed unsent because they were out of date by the time they could go.
    pub expired: usize,
    /// Why nothing (or not everything) was attempted, e.g. SMS is switched off.
    pub skipped_reason: Option<String>,
}

const EXPIRED_REASON: &str =
    "Not sent: by the time it could be sent, this message was out of date.";

/// Messages read from the queue per round. The queue is read again between
/// rounds, so an urgent message written mid-broadcast goes next.
const BATCH: usize = 50;

/// Lets only one sender run at a time. Two at once would both read the same
/// queued rows and send each message twice: two texts to one parent, and the
/// school pays for both.
pub struct Gate {
    running: AtomicBool,
    /// Set by a caller that found a sender running: look again before stopping.
    rerun: AtomicBool,
}

impl Gate {
    pub const fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            rerun: AtomicBool::new(false),
        }
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

/// The gate every real send goes through.
static GATE: Gate = Gate::new();

/// Clears `running` even if sending panics, so one bad message cannot
/// silence SMS until the application is restarted.
struct RunningGuard<'a>(&'a Gate);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::SeqCst);
    }
}

/// Anything that can deliver one message: the real gateways, or a stand-in
/// in tests.
pub type Deliver<'a> = dyn Fn(&SmsSettings, &str, &str) -> Result<String, SendError> + Sync + 'a;

/// Sends everything queued, most urgent first. Only one sender ever runs at
/// a time; a call that finds one running asks it to go round again and
/// returns at once, so nothing waits for the next scheduled round.
pub fn flush(state: &AppState) -> AppResult<FlushResult> {
    flush_with(&GATE, &state.db, &send)
}

pub fn flush_with(gate: &Gate, db: &Database, deliver: &Deliver<'_>) -> AppResult<FlushResult> {
    let mut total = FlushResult::default();
    loop {
        if gate.running.swap(true, Ordering::SeqCst) {
            gate.rerun.store(true, Ordering::SeqCst);
            // The running sender may have checked `rerun` and stopped between
            // our two steps. If it has, the job is ours.
            if gate.running.swap(true, Ordering::SeqCst) {
                total
                    .skipped_reason
                    .get_or_insert_with(|| "Already sending. New messages go in this round.".into());
                return Ok(total);
            }
        }
        let guard = RunningGuard(gate);
        gate.rerun.store(false, Ordering::SeqCst);
        let outcome = drain(db, &mut total, deliver);
        drop(guard);
        outcome?;
        if !gate.rerun.swap(false, Ordering::SeqCst) {
            return Ok(total);
        }
    }
}

/// One sender's work. The database lock is held only to read the queue and
/// to record each result, never across a network call.
fn drain(db: &Database, total: &mut FlushResult, deliver: &Deliver<'_>) -> AppResult<()> {
    let settings = {
        let conn = db.lock();
        total.expired += conn.execute(
            "UPDATE sms_outbox SET status = 'failed', last_error = ?1
             WHERE status = 'queued' AND expires_at IS NOT NULL AND expires_at <= ?2",
            params![EXPIRED_REASON, stamp(Utc::now())],
        )?;
        (load_settings(&conn)?, crate::network::offline_only(&conn)?)
    };
    let (settings, offline_only) = settings;

    let configured =
        settings.provider != "off" && !settings.username.is_empty() && !settings.api_key.is_empty();

    if offline_only {
        total.skipped_reason = Some(crate::network::OFFLINE_ONLY_MESSAGE.into());
    } else if configured {
        'rounds: loop {
            let batch: Vec<(String, String, String)> = {
                let conn = db.lock();
                let mut stmt = conn.prepare(
                    "SELECT id, to_phone, body FROM sms_outbox
                     WHERE status = 'queued' AND (expires_at IS NULL OR expires_at > ?1)
                     ORDER BY priority ASC, created_at ASC
                     LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(params![stamp(Utc::now()), BATCH as i64], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                rows
            };
            let full = batch.len() == BATCH;

            for (id, to, body) in batch {
                let outcome = deliver(&settings, &to, &body);
                let conn = db.lock();
                let now = Utc::now().to_rfc3339();
                match outcome {
                    Ok(reference) => {
                        conn.execute(
                            "UPDATE sms_outbox SET status = 'sent', attempts = attempts + 1,
                                    provider_ref = ?1, last_error = NULL,
                                    last_attempt_at = ?2, sent_at = ?2
                             WHERE id = ?3",
                            params![reference, now, id],
                        )?;
                        total.sent += 1;
                    }
                    Err(SendError::Offline(reason)) => {
                        // No internet or the gateway is down. The message keeps
                        // its place and spends no attempt; trying the rest now
                        // would only wait out the same timeout for each one.
                        conn.execute(
                            "UPDATE sms_outbox SET last_error = ?1, last_attempt_at = ?2 WHERE id = ?3",
                            params![reason, now, id],
                        )?;
                        break 'rounds;
                    }
                    Err(SendError::Rejected(reason)) => {
                        conn.execute(
                            "UPDATE sms_outbox SET status = 'failed', attempts = attempts + 1,
                                    last_error = ?1, last_attempt_at = ?2
                             WHERE id = ?3",
                            params![reason, now, id],
                        )?;
                        total.failed += 1;
                    }
                }
            }

            if !full {
                break;
            }
        }
    } else {
        total.skipped_reason =
            Some("SMS is not set up yet. Messages are waiting in the outbox.".into());
    }

    let conn = db.lock();
    if let Some((amount, currency)) = take_reported_balance() {
        store_balance(&conn, amount, &currency)?;
    }
    total.still_queued = conn.query_row(
        "SELECT COUNT(*) FROM sms_outbox WHERE status = 'queued'",
        [],
        |row| row.get::<_, i64>(0),
    )? as usize;
    Ok(())
}

/// Wakes the background sender. Commands call this after queueing a message
/// instead of holding the application handle themselves, so they stay plain
/// functions of the database (and can be exercised without a window).
static WAKE: (std::sync::Mutex<bool>, std::sync::Condvar) =
    (std::sync::Mutex::new(false), std::sync::Condvar::new());

/// Asks the sender to go now rather than at its next round.
pub fn wake() {
    let (flag, signal) = &WAKE;
    if let Ok(mut woken) = flag.lock() {
        *woken = true;
        signal.notify_all();
    }
}

/// Sends the outbox every couple of minutes for as long as Termbook is
/// open, and at once whenever [`wake`] is called. Runs on its own
/// thread, so a screen never waits on the network.
pub fn spawn_scheduler(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("rm-sms".into())
        .spawn(move || loop {
            {
                let (flag, signal) = &WAKE;
                let Ok(woken) = flag.lock() else { return };
                let mut woken = match signal.wait_timeout_while(woken, INTERVAL, |w| !*w) {
                    Ok((guard, _)) => guard,
                    Err(_) => return,
                };
                *woken = false;
            }
            let state = app.state::<AppState>();
            if let Err(err) = flush(&state) {
                log::error!("sending queued SMS failed: {err}");
            }
        });
    if let Err(err) = spawned {
        log::error!("could not start the SMS sender: {err}");
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
        "egosms" => send_egosms_comms(settings, to, body),
        "egosms_classic" => send_egosms(settings, to, body),
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

// --- EgoSMS Comms: the JSON API of comms.egosms.co ---------------------------
//
// As Pahappa's own SDK (github.com/Pahappa-LTD/comms-go-sdk) speaks it:
// POST {"method": "SendSms" | "Balance", "userdata": {"username", "password"
// (the API key)}, "msgdata": [...], "walletType": "Local"}; the reply is
// {"Status": "OK" | "Failed", "Message", "Cost", "MsgFollowUpUniqueCode",
// "Balance"}. Every reply carries the balance, so each text sent also tells
// us how much credit is left.

const EGOSMS_COMMS_URL: &str = "https://comms.egosms.co/api/v1/json";

fn egosms_comms(request: serde_json::Value) -> Result<serde_json::Value, SendError> {
    let response = agent()
        .post(EGOSMS_COMMS_URL)
        .set("Accept", "application/json")
        .send_json(request)
        .map_err(classify)?;
    let text = response.into_string().map_err(|err| SendError::Offline(err.to_string()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| SendError::Rejected(format!("Unexpected reply from EgoSMS: {}", text.trim())))?;
    if let Some(balance) = json["Balance"].as_f64() {
        remember_balance(balance, "UGX");
    }
    if json["Status"].as_str() == Some("OK") {
        Ok(json)
    } else {
        Err(SendError::Rejected(format!(
            "EgoSMS: {}",
            json["Message"].as_str().unwrap_or("the message was refused")
        )))
    }
}

fn send_egosms_comms(settings: &SmsSettings, to: &str, body: &str) -> Result<String, SendError> {
    let sender = if settings.sender_id.is_empty() { "EgoSMS" } else { settings.sender_id.as_str() };
    let reply = egosms_comms(
        serde_json::json!({
            "method": "SendSms",
            "userdata": { "username": settings.username, "password": settings.api_key },
            "msgdata": [{
                "number": to.trim_start_matches('+'),
                "message": body,
                "senderid": sender,
                "priority": "0",
            }],
            "walletType": "Local",
        }),
    )?;
    Ok(reply["MsgFollowUpUniqueCode"].as_str().unwrap_or("").to_string())
}

// --- What credit is left ---------------------------------------------------

/// The last balance a gateway told us, set by whichever thread heard it.
static LAST_BALANCE: std::sync::Mutex<Option<(f64, String)>> = std::sync::Mutex::new(None);

fn remember_balance(amount: f64, currency: &str) {
    if let Ok(mut last) = LAST_BALANCE.lock() {
        *last = Some((amount, currency.to_string()));
    }
}

/// Takes the balance the gateway last reported, if any arrived since.
pub fn take_reported_balance() -> Option<(f64, String)> {
    LAST_BALANCE.lock().ok().and_then(|mut last| last.take())
}

/// Asks the gateway for the credit left. Network: never call with the
/// database lock held.
pub fn fetch_balance(settings: &SmsSettings) -> Result<(f64, String), SendError> {
    match settings.provider.as_str() {
        "egosms" => {
            let reply = egosms_comms(
                serde_json::json!({
                    "method": "Balance",
                    "userdata": { "username": settings.username, "password": settings.api_key },
                    "walletType": "Local",
                }),
            )?;
            reply["Balance"]
                .as_f64()
                .map(|b| (b, "UGX".to_string()))
                .ok_or_else(|| SendError::Rejected("EgoSMS did not say the balance.".into()))
        }
        "africastalking" => {
            let url = if settings.username == "sandbox" {
                "https://api.sandbox.africastalking.com/version1/user"
            } else {
                "https://api.africastalking.com/version1/user"
            };
            let response = agent()
                .get(url)
                .query("username", &settings.username)
                .set("apiKey", &settings.api_key)
                .set("Accept", "application/json")
                .call()
                .map_err(classify)?;
            let text = response.into_string().map_err(|err| SendError::Offline(err.to_string()))?;
            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|_| SendError::Rejected(format!("Unexpected reply from Africa's Talking: {text}")))?;
            // "UGX 1,234.5678"
            let raw = json["UserData"]["balance"].as_str().unwrap_or_default();
            parse_money(raw).ok_or_else(|| SendError::Rejected(format!("Africa's Talking said: {raw}")))
        }
        "egosms_classic" => Err(SendError::Rejected(
            "The older EgoSMS API does not report the balance. Check it on egosms.co.".into(),
        )),
        _ => Err(SendError::Rejected("SMS is switched off.".into())),
    }
}

/// "UGX 1,234.5678" → (1234.5678, "UGX").
pub fn parse_money(raw: &str) -> Option<(f64, String)> {
    let currency: String = raw.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let number: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    number.parse().ok().map(|n| (n, if currency.is_empty() { "UGX".into() } else { currency }))
}

/// Writes a balance the gateway reported to the settings, with the time.
pub fn store_balance(conn: &Connection, amount: f64, currency: &str) -> AppResult<()> {
    repo::set_setting(conn, "sms.balance", &format!("{amount}"))?;
    repo::set_setting(conn, "sms.balance_currency", currency)?;
    repo::set_setting(conn, "sms.balance_at", &Utc::now().to_rfc3339())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsBalance {
    pub amount: Option<f64>,
    pub currency: String,
    /// When the gateway last said it; none if it never has.
    pub checked_at: Option<String>,
    /// Below the school's warning level.
    pub low: bool,
    pub warn_below: f64,
    /// Roughly how many more texts, at the school's price per SMS.
    pub texts_left: Option<i64>,
}

pub fn stored_balance(conn: &Connection) -> AppResult<SmsBalance> {
    let amount: Option<f64> = repo::get_setting(conn, "sms.balance")?.and_then(|v| v.parse().ok());
    let warn_below: f64 = repo::get_setting(conn, "sms.balance_warn")?.and_then(|v| v.parse().ok()).unwrap_or(5_000.0);
    let price: f64 = repo::get_setting(conn, "sms.price")?.and_then(|v| v.parse().ok()).unwrap_or(35.0);
    Ok(SmsBalance {
        amount,
        currency: repo::get_setting(conn, "sms.balance_currency")?.unwrap_or_else(|| "UGX".into()),
        checked_at: repo::get_setting(conn, "sms.balance_at")?,
        low: amount.is_some_and(|a| a < warn_below),
        warn_below,
        texts_left: amount.filter(|_| price > 0.0).map(|a| (a / price).floor() as i64),
    })
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
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn balances_are_read_from_how_gateways_write_them() {
        assert_eq!(parse_money("UGX 1,234.5678"), Some((1234.5678, "UGX".into())));
        assert_eq!(parse_money("KES 0.00"), Some((0.0, "KES".into())));
        assert_eq!(parse_money("12000"), Some((12000.0, "UGX".into())));
        assert_eq!(parse_money("n/a"), None);
    }

    #[test]
    fn a_reported_balance_is_kept_with_how_many_texts_it_buys() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.lock();
        assert_eq!(stored_balance(&conn).unwrap().amount, None);
        store_balance(&conn, 4_200.0, "UGX").unwrap();
        let balance = stored_balance(&conn).unwrap();
        assert_eq!((balance.amount, balance.low, balance.texts_left), (Some(4_200.0), true, Some(120)));
        repo::set_setting(&conn, "sms.balance_warn", "1000").unwrap();
        assert!(!stored_balance(&conn).unwrap().low);
    }

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

    // --- The outbox and the sender -----------------------------------------

    use std::collections::HashMap;
    use std::sync::{mpsc, Arc, Mutex};

    fn configured_db() -> Database {
        let db = Database::open_in_memory().expect("open");
        {
            let conn = db.lock();
            repo::set_setting(&conn, "sms.provider", "africastalking").unwrap();
            repo::set_setting(&conn, "sms.username", "school").unwrap();
            repo::set_setting(&conn, "sms.api_key", "key").unwrap();
        }
        db
    }

    fn put(db: &Database, to: &str, kind: &str) -> String {
        insert(&db.lock(), to, "hello", kind, None, None).unwrap()
    }

    fn row(db: &Database, id: &str) -> (String, i64, Option<String>) {
        db.lock()
            .query_row(
                "SELECT status, attempts, last_error FROM sms_outbox WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }

    fn offline(_: &SmsSettings, _: &str, _: &str) -> Result<String, SendError> {
        Err(SendError::Offline("no internet".into()))
    }

    fn delivered(_: &SmsSettings, _: &str, _: &str) -> Result<String, SendError> {
        Ok("ref".into())
    }

    #[test]
    fn a_long_outage_never_strands_a_message() {
        // The old rule gave up silently after eight offline tries, about
        // sixteen minutes. A school is often offline for a whole morning.
        let gate = Gate::new();
        let db = configured_db();
        let id = put(&db, "+256772000001", "bulk");

        for _ in 0..200 {
            let result = flush_with(&gate, &db, &offline).unwrap();
            assert_eq!(result.still_queued, 1);
        }
        let (status, attempts, error) = row(&db, &id);
        assert_eq!((status.as_str(), attempts), ("queued", 0), "offline spends no attempt");
        assert_eq!(error.as_deref(), Some("no internet"));

        let result = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!((result.sent, result.still_queued), (1, 0));
        assert_eq!(row(&db, &id).0, "sent");
    }

    #[test]
    fn offline_only_never_touches_the_network_and_queues_nothing() {
        let gate = Gate::new();
        let db = configured_db();
        let waiting = put(&db, "+256772000001", "bulk");
        crate::network::set_offline_only(&db.lock(), true).unwrap();

        let calls = AtomicUsize::new(0);
        let counting = |_: &SmsSettings, _: &str, _: &str| -> Result<String, SendError> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok("ref".into())
        };
        let result = flush_with(&gate, &db, &counting).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "not one call to the gateway");
        assert!(result.skipped_reason.unwrap().contains("offline only"));
        assert_eq!(row(&db, &waiting).0, "queued");

        // New messages are refused outright, or skipped where a text is a
        // side effect of something else (a pass-out still gets issued).
        assert!(insert(&db.lock(), "+256772000002", "hi", "bulk", None, None).is_err());
        assert_eq!(
            queue(&db.lock(), Some("0772000003"), "hi", "pass_out", "pas_1", None).unwrap(),
            None
        );
    }

    #[test]
    fn being_offline_stops_the_round_at_the_first_message() {
        // Otherwise 800 queued messages each wait out the network timeout.
        let gate = Gate::new();
        let db = configured_db();
        for n in 0..5 {
            put(&db, &format!("+25677200000{n}"), "bulk");
        }
        let calls = AtomicUsize::new(0);
        let counting = |_: &SmsSettings, _: &str, _: &str| -> Result<String, SendError> {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(SendError::Offline("no internet".into()))
        };
        let result = flush_with(&gate, &db, &counting).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.still_queued, 5);
    }

    #[test]
    fn a_rejected_message_fails_for_good_and_the_rest_still_go() {
        let gate = Gate::new();
        let db = configured_db();
        let bad = put(&db, "+256772000001", "bulk");
        let good = put(&db, "+256772000002", "bulk");
        let picky = |_: &SmsSettings, to: &str, _: &str| -> Result<String, SendError> {
            if to.ends_with('1') {
                Err(SendError::Rejected("invalid number".into()))
            } else {
                Ok("ref".into())
            }
        };
        let result = flush_with(&gate, &db, &picky).unwrap();
        assert_eq!((result.sent, result.failed), (1, 1));
        assert_eq!(row(&db, &bad).0, "failed");
        assert_eq!(row(&db, &good).0, "sent");
    }

    #[test]
    fn urgent_messages_jump_the_bulk_queue() {
        let gate = Gate::new();
        let db = configured_db();
        for n in 0..3 {
            put(&db, &format!("+25677200000{n}"), "bulk");
        }
        put(&db, "+256772999999", "pass_out");

        let order = Mutex::new(Vec::new());
        let recording = |_: &SmsSettings, to: &str, _: &str| -> Result<String, SendError> {
            order.lock().unwrap().push(to.to_string());
            Ok("ref".into())
        };
        flush_with(&gate, &db, &recording).unwrap();
        assert_eq!(order.lock().unwrap()[0], "+256772999999");
    }

    #[test]
    fn a_big_broadcast_goes_out_in_one_flush() {
        // Batches of 50 used to mean one batch per two-minute round.
        let gate = Gate::new();
        let db = configured_db();
        for n in 0..(BATCH * 2 + 7) {
            put(&db, &format!("+256772{n:06}"), "bulk");
        }
        let result = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!((result.sent, result.still_queued), (BATCH * 2 + 7, 0));
    }

    #[test]
    fn an_out_of_date_message_is_closed_not_sent() {
        let gate = Gate::new();
        let db = configured_db();
        let id = put(&db, "+256772000001", "pass_out");
        db.lock()
            .execute(
                "UPDATE sms_outbox SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [&id],
            )
            .unwrap();

        let result = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!((result.sent, result.expired), (0, 1));
        let (status, _, error) = row(&db, &id);
        assert_eq!(status, "failed");
        assert!(error.unwrap().contains("out of date"));
    }

    #[test]
    fn pass_outs_expire_and_broadcasts_do_not() {
        assert_eq!(policy("pass_out"), (PRIORITY_URGENT, Some(chrono::Duration::hours(12))));
        assert_eq!(policy("absence").1, Some(chrono::Duration::hours(12)));
        assert_eq!(policy("bulk"), (PRIORITY_BULK, None));
    }

    #[test]
    fn nothing_is_sent_until_sms_is_set_up() {
        let gate = Gate::new();
        let db = Database::open_in_memory().unwrap();
        put(&db, "+256772000001", "bulk");
        let result = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!((result.sent, result.still_queued), (0, 1));
        assert!(result.skipped_reason.is_some());
    }

    #[test]
    fn two_senders_never_text_a_parent_twice() {
        // A pass-out starts a sender while another is mid-way through the
        // queue. Before the gate, both read the same rows.
        let gate = Arc::new(Gate::new());
        let db = Arc::new(configured_db());
        put(&db, "+256772000001", "pass_out");

        let sends: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
        let (entered_tx, entered_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let entered_tx = Mutex::new(entered_tx);

        let first = {
            let (gate, db, sends) = (gate.clone(), db.clone(), sends.clone());
            std::thread::spawn(move || {
                let slow = |_: &SmsSettings, to: &str, _: &str| -> Result<String, SendError> {
                    *sends.lock().unwrap().entry(to.to_string()).or_default() += 1;
                    let _ = entered_tx.lock().unwrap().send(());
                    let _ = release_rx.lock().unwrap().recv();
                    Ok("ref".into())
                };
                flush_with(&gate, &db, &slow).unwrap()
            })
        };

        // The first sender is now inside a network call. A second pass-out
        // arrives and starts its own sender.
        entered_rx.recv().unwrap();
        put(&db, "+256772000002", "pass_out");
        let second = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!(second.sent, 0, "the second sender must not run alongside");
        assert!(second.skipped_reason.is_some());

        // Release the first sender; it must go round again for the newcomer.
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        let first = first.join().unwrap();
        assert_eq!(first.sent, 2);

        let sends = sends.lock().unwrap();
        assert_eq!(sends.get("+256772000001"), Some(&1));
        assert_eq!(sends.get("+256772000002"), Some(&1));
    }

    #[test]
    fn a_panicking_sender_does_not_silence_sms() {
        let gate = Gate::new();
        let db = configured_db();
        put(&db, "+256772000001", "bulk");
        let boom = |_: &SmsSettings, _: &str, _: &str| -> Result<String, SendError> {
            panic!("gateway library bug")
        };
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            flush_with(&gate, &db, &boom)
        }));
        assert!(crashed.is_err());
        let result = flush_with(&gate, &db, &delivered).unwrap();
        assert_eq!(result.sent, 1, "the gate was released by the panic");
    }

    #[test]
    fn one_curly_apostrophe_would_make_a_message_unicode() {
        let typed = "Parents\u{2019} meeting on Friday \u{2013} 2pm";
        assert_eq!(cost(typed).0, Encoding::Unicode);
        let plain = plain_text(typed);
        assert_eq!(plain, "Parents' meeting on Friday - 2pm");
        assert_eq!(cost(&plain).0, Encoding::Gsm);
    }

    #[test]
    fn unicode_parts_are_seventy_and_sixty_seven() {
        let emoji = "\u{1F600}"; // two UTF-16 units
        assert_eq!(cost(&"\u{0101}".repeat(70)), (Encoding::Unicode, 1));
        assert_eq!(cost(&"\u{0101}".repeat(71)), (Encoding::Unicode, 2));
        assert_eq!(cost(&"\u{0101}".repeat(134)), (Encoding::Unicode, 2));
        assert_eq!(cost(&"\u{0101}".repeat(135)), (Encoding::Unicode, 3));
        assert_eq!(cost(&emoji.repeat(35)), (Encoding::Unicode, 1));
    }

    #[test]
    fn extended_gsm_characters_take_two_places() {
        assert_eq!(cost(&"€".repeat(80)), (Encoding::Gsm, 1));
        assert_eq!(cost(&"€".repeat(81)), (Encoding::Gsm, 2));
        assert_eq!(cost("UGX 50,000 [fees]"), (Encoding::Gsm, 1));
    }

    #[test]
    fn segments_follow_the_gsm_rule() {
        assert_eq!(segments(&"a".repeat(160)), 1);
        assert_eq!(segments(&"a".repeat(161)), 2);
        assert_eq!(segments(&"a".repeat(306)), 2);
        assert_eq!(segments(&"a".repeat(307)), 3);
    }
}
