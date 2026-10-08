//! FR-G23 — bulk SMS to parents and staff, and the learner groups it can
//! be sent to.
//!
//! A message is written once and sent to an audience built from parts: the
//! whole school, classes, streams, groups (the football team, the choir) and
//! staff. Every part is checked against who is asking — a Class Teacher may
//! text their own class, a teacher in charge of a group may text that group,
//! and only a School Admin may text the whole school or the staff.
//!
//! Three things keep the school's SMS bill honest:
//!
//! * **One text per phone number.** A parent with three children in the
//!   school gets one message, not three. `{learner}` becomes "Amina, Musa and
//!   Sarah" for that parent instead of three separate texts.
//! * **The real cost before sending.** Curly quotes and long dashes are
//!   turned plain first (one would make the message Unicode and can double
//!   the price), and the preview counts every SMS the gateway will charge.
//! * **No surprise at send time.** Sending repeats the preview's count; if
//!   the audience changed in between, nothing is sent and the sender is told.
//!
//! Copies go through the ordinary outbox at bulk priority, so a pass-out
//! written during a broadcast still goes first.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::audit;
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::sms::{self, Encoding};
use crate::state::AppState;

const BROADCAST_PREFIX: &str = "bc";
const GROUP_PREFIX: &str = "grp";

/// The longest message a broadcast may be, in SMS per recipient. Past this a
/// school is paying for a letter, which belongs on paper.
const MAX_PARTS: usize = 4;

/// How many names the preview lists for learners with no usable phone.
const MISSING_SHOWN: usize = 40;

const PLACEHOLDERS: &[&str] = &["learner", "class", "name"];

// ---------------------------------------------------------------------------
// Audiences
// ---------------------------------------------------------------------------

/// One part of an audience. `id` is the class, stream or group; it is empty
/// for `school` and `staff`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudiencePart {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
}

impl AudiencePart {
    fn id(&self) -> AppResult<&str> {
        self.id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| AppError::validation(format!("Pick which {} to text.", self.kind)))
    }
}

/// Whether this person may text this part of the school. Every audience
/// passes through here and nowhere else, so who may text whom is decided in
/// one place.
fn authorize(conn: &Connection, session: &Session, part: &AudiencePart) -> AppResult<()> {
    if session.has(Permission::SendMessages) {
        return Ok(());
    }

    let teachers_may = repo::get_setting(conn, "sms.teachers_may_broadcast")?
        .map(|v| v != "0")
        .unwrap_or(true);
    if !teachers_may {
        return Err(AppError::validation(
            "Only the school office can send messages to parents. Ask a School Admin.",
        ));
    }

    match part.kind.as_str() {
        "class" => session.require_manage_class(part.id()?),
        "stream" => {
            let class_id: String = conn
                .query_row(
                    "SELECT class_id FROM streams WHERE id = ?1",
                    params![part.id()?],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("stream"))?;
            session.require_manage_class(&class_id)
        }
        "group" => {
            let leader: Option<String> = conn
                .query_row(
                    "SELECT leader_id FROM learner_groups WHERE id = ?1 AND status = 'active'",
                    params![part.id()?],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("group"))?;
            if leader.as_deref() == Some(session.user_id.as_str()) {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        "school" | "staff" => Err(AppError::Forbidden),
        other => Err(AppError::validation(format!("Unknown audience \"{other}\"."))),
    }
}

/// A learner whose guardian will be texted.
#[derive(Debug, Clone)]
struct Learner {
    id: String,
    name: String,
    class_name: String,
    phone: Option<String>,
}

/// Everyone one message will reach, one entry per phone number.
#[derive(Debug, Default)]
struct Resolved {
    /// Phone number to the learners whose guardian uses it, in order.
    guardians: BTreeMap<String, Vec<Learner>>,
    /// Phone number to staff name.
    staff: BTreeMap<String, String>,
    /// Learners in the audience with no number that can receive an SMS.
    missing: Vec<Learner>,
    /// Staff in the audience with no usable number.
    staff_missing: Vec<String>,
    labels: Vec<String>,
}

const LEARNER_SELECT: &str = "SELECT s.id, s.full_name, c.name, s.guardian_phone
     FROM students s
     JOIN enrollments e ON e.student_id = s.id AND e.status = 'active'
                       AND (?1 IS NULL OR e.academic_year_id = ?1)
     JOIN classes c ON c.id = e.class_id";

fn learners(conn: &Connection, sql: &str, year: &Option<String>, id: Option<&str>) -> AppResult<Vec<Learner>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![year, id], |r| {
            Ok(Learner {
                id: r.get(0)?,
                name: r.get(1)?,
                class_name: r.get(2)?,
                phone: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn resolve(conn: &Connection, parts: &[AudiencePart]) -> AppResult<Resolved> {
    let year = repo::current_academic_year_id(conn)?;
    let mut seen_learners = BTreeSet::new();
    let mut seen_parts = BTreeSet::new();
    let mut resolved = Resolved::default();
    let mut include_staff = false;
    let mut found: Vec<Learner> = Vec::new();

    for part in parts {
        if !seen_parts.insert((part.kind.clone(), part.id.clone().unwrap_or_default())) {
            continue;
        }
        let (rows, label) = match part.kind.as_str() {
            "school" => (
                learners(
                    conn,
                    &format!("{LEARNER_SELECT} WHERE s.status = 'active' AND ?2 IS NULL ORDER BY c.ladder_position, s.full_name"),
                    &year,
                    None,
                )?,
                "the whole school".to_string(),
            ),
            "class" => {
                let id = part.id()?;
                let name: String = conn
                    .query_row("SELECT name FROM classes WHERE id = ?1", params![id], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("class"))?;
                (
                    learners(
                        conn,
                        &format!("{LEARNER_SELECT} WHERE s.status = 'active' AND e.class_id = ?2 ORDER BY s.full_name"),
                        &year,
                        Some(id),
                    )?,
                    name,
                )
            }
            "stream" => {
                let id = part.id()?;
                let name: String = conn
                    .query_row(
                        "SELECT c.name || ' ' || st.name FROM streams st JOIN classes c ON c.id = st.class_id
                         WHERE st.id = ?1",
                        params![id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| AppError::not_found("stream"))?;
                (
                    learners(
                        conn,
                        &format!("{LEARNER_SELECT} WHERE s.status = 'active' AND e.stream_id = ?2 ORDER BY s.full_name"),
                        &year,
                        Some(id),
                    )?,
                    name,
                )
            }
            "group" => {
                let id = part.id()?;
                let name: String = conn
                    .query_row(
                        "SELECT name FROM learner_groups WHERE id = ?1 AND status = 'active'",
                        params![id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| AppError::not_found("group"))?;
                (
                    learners(
                        conn,
                        &format!(
                            "{LEARNER_SELECT}
                             JOIN learner_group_members m ON m.student_id = s.id
                                  AND m.group_id = ?2 AND m.status = 'active'
                             WHERE s.status = 'active' ORDER BY s.full_name"
                        ),
                        &year,
                        Some(id),
                    )?,
                    format!("the {name} group"),
                )
            }
            "staff" => {
                include_staff = true;
                (Vec::new(), "all staff".to_string())
            }
            other => return Err(AppError::validation(format!("Unknown audience \"{other}\"."))),
        };
        resolved.labels.push(label);
        for learner in rows {
            if seen_learners.insert(learner.id.clone()) {
                found.push(learner);
            }
        }
    }

    for learner in found {
        match learner.phone.as_deref().and_then(sms::normalize_phone) {
            Some(phone) => resolved.guardians.entry(phone).or_default().push(learner),
            None => resolved.missing.push(learner),
        }
    }

    if include_staff {
        let mut stmt = conn.prepare(
            "SELECT full_name, phone FROM users WHERE status = 'active' ORDER BY full_name",
        )?;
        let staff = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, phone) in staff {
            match phone.as_deref().and_then(sms::normalize_phone) {
                // A member of staff who is also a parent gets the staff copy
                // only; their guardian copy is dropped below.
                Some(phone) => {
                    resolved.staff.entry(phone).or_insert(name);
                }
                None => resolved.staff_missing.push(name),
            }
        }
    }

    Ok(resolved)
}

/// "Amina", "Amina and Musa", "Amina, Musa and Sarah".
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// "P5, P6 and the Football team".
fn audience_label(labels: &[String]) -> String {
    join_names(labels)
}

/// Every `{word}` in the template must be one this module fills in; a typo
/// such as `{leaner}` would otherwise reach 800 parents as typed.
fn check_placeholders(template: &str) -> AppResult<BTreeSet<String>> {
    let mut used = BTreeSet::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(AppError::validation(
                "A { has no matching }. Placeholders look like {learner}.",
            ));
        };
        let word = after[..close].trim().to_lowercase();
        if !PLACEHOLDERS.contains(&word.as_str()) {
            return Err(AppError::validation(format!(
                "{{{}}} is not something Phantom can fill in. Use {{learner}}, {{class}} or {{name}}.",
                &after[..close]
            )));
        }
        used.insert(word);
        rest = &after[close + 1..];
    }
    Ok(used)
}

fn fill(template: &str, learner: &str, class: &str, name: &str) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}').unwrap_or(after.len());
        match after[..close].trim().to_lowercase().as_str() {
            "learner" => out.push_str(learner),
            "class" => out.push_str(class),
            "name" => out.push_str(name),
            _ => {}
        }
        rest = after.get(close + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// One finished message, ready for the outbox.
struct Outgoing {
    to: String,
    body: String,
}

/// Builds every message. Placeholders are filled per phone number, never per
/// learner, so a parent of three gets one text naming all three.
fn compose(signature: &str, template: &str, resolved: &Resolved) -> AppResult<Vec<Outgoing>> {
    let used = check_placeholders(template)?;
    // Typographic characters are made plain on the finished message below.
    let template = template.trim();
    if template.is_empty() {
        return Err(AppError::validation("Write the message first."));
    }
    if !resolved.staff.is_empty() && (used.contains("learner") || used.contains("class")) {
        return Err(AppError::validation(
            "{learner} and {class} only work for parents. Send staff their own message.",
        ));
    }
    if !resolved.guardians.is_empty() && used.contains("name") {
        return Err(AppError::validation(
            "{name} is a staff member's name. For parents, use {learner}.",
        ));
    }

    let signature = sms::plain_text(signature);
    let mut out = Vec::new();
    for (phone, kids) in &resolved.guardians {
        if resolved.staff.contains_key(phone) {
            continue;
        }
        let names: Vec<String> = kids.iter().map(|k| k.name.clone()).collect();
        let mut classes: Vec<String> = Vec::new();
        for kid in kids {
            if !classes.contains(&kid.class_name) {
                classes.push(kid.class_name.clone());
            }
        }
        let text = fill(template, &join_names(&names), &join_names(&classes), "");
        out.push(Outgoing {
            to: phone.clone(),
            body: sms::plain_text(&format!("{signature}: {text}")),
        });
    }
    for (phone, name) in &resolved.staff {
        let text = fill(template, "", "", name);
        out.push(Outgoing {
            to: phone.clone(),
            body: sms::plain_text(&format!("{signature}: {text}")),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// What a sender may choose from
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudienceStream {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudienceClass {
    pub id: String,
    pub name: String,
    pub learners: i64,
    pub streams: Vec<AudienceStream>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudienceGroup {
    pub id: String,
    pub name: String,
    pub members: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudienceOptions {
    pub can_text_school: bool,
    pub can_text_staff: bool,
    pub classes: Vec<AudienceClass>,
    pub groups: Vec<AudienceGroup>,
    /// Whether SMS is set up at all; messages still queue when it is not.
    pub sms_ready: bool,
    /// Why this person has nothing to choose from, when they have nothing.
    pub blocked_reason: Option<String>,
}

/// The parts of the school this person may text.
#[tauri::command]
pub fn list_sms_audiences(state: State<'_, AppState>) -> AppResult<AudienceOptions> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let year = repo::current_academic_year_id(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.name,
                (SELECT COUNT(*) FROM enrollments e JOIN students s ON s.id = e.student_id
                  WHERE e.class_id = c.id AND e.status = 'active' AND s.status = 'active'
                    AND (?1 IS NULL OR e.academic_year_id = ?1))
         FROM classes c WHERE c.status = 'active' ORDER BY c.ladder_position",
    )?;
    let all_classes = stmt
        .query_map(params![year], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut classes = Vec::new();
    for (id, name, count) in all_classes {
        let part = AudiencePart { kind: "class".into(), id: Some(id.clone()) };
        if authorize(&conn, &session, &part).is_err() {
            continue;
        }
        let mut stmt = conn.prepare(
            "SELECT id, name FROM streams WHERE class_id = ?1 AND status = 'active' ORDER BY name",
        )?;
        let streams = stmt
            .query_map(params![id], |r| Ok(AudienceStream { id: r.get(0)?, name: r.get(1)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        classes.push(AudienceClass { id, name, learners: count, streams });
    }

    let mut stmt = conn.prepare(
        "SELECT g.id, g.name,
                (SELECT COUNT(*) FROM learner_group_members m JOIN students s ON s.id = m.student_id
                  WHERE m.group_id = g.id AND m.status = 'active' AND s.status = 'active')
         FROM learner_groups g
         WHERE g.status = 'active' AND (?1 OR g.leader_id = ?2)
         ORDER BY g.name COLLATE NOCASE",
    )?;
    let groups = stmt
        .query_map(params![session.has(Permission::SendMessages), session.user_id], |r| {
            Ok(AudienceGroup { id: r.get(0)?, name: r.get(1)?, members: r.get(2)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let groups = groups
        .into_iter()
        .filter(|g| {
            authorize(&conn, &session, &AudiencePart { kind: "group".into(), id: Some(g.id.clone()) })
                .is_ok()
        })
        .collect::<Vec<_>>();

    let settings = sms::load_settings(&conn)?;
    let sms_ready = settings.provider != "off" && !settings.api_key.is_empty();

    let blocked_reason = if classes.is_empty() && groups.is_empty() && !session.has(Permission::SendMessages) {
        let check = authorize(&conn, &session, &AudiencePart { kind: "school".into(), id: None });
        Some(match check {
            Err(AppError::Validation(msg)) => msg,
            _ => "You can text the parents of a class you are Class Teacher of, or a group you are in charge of. You have neither yet.".into(),
        })
    } else {
        None
    };

    Ok(AudienceOptions {
        can_text_school: session.has(Permission::SendMessages),
        can_text_staff: session.has(Permission::SendMessages),
        classes,
        groups,
        sms_ready,
        blocked_reason,
    })
}

// ---------------------------------------------------------------------------
// Preview and send
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastPreview {
    pub audience_label: String,
    /// Phone numbers that will be texted.
    pub recipients: usize,
    /// Learners covered, counting each child once.
    pub learners: usize,
    /// SMS the gateway will charge for, across every recipient.
    pub units: usize,
    /// The longest single message, in SMS.
    pub parts_per_message: usize,
    pub encoding: Encoding,
    /// One finished message exactly as a parent will read it.
    pub sample: Option<String>,
    /// Learners and staff in the audience with no number that can be texted.
    pub missing_count: usize,
    pub missing_names: Vec<String>,
    /// Plain-language notes: Unicode cost, long message, shared phones.
    pub notes: Vec<String>,
}

struct Plan {
    preview: BroadcastPreview,
    outgoing: Vec<Outgoing>,
}

fn plan(conn: &Connection, session: &Session, audience: &[AudiencePart], template: &str) -> AppResult<Plan> {
    if audience.is_empty() {
        return Err(AppError::validation("Choose who to text."));
    }
    for part in audience {
        authorize(conn, session, part)?;
    }
    let resolved = resolve(conn, audience)?;
    let signature = sms::signature(conn)?;
    let outgoing = compose(&signature, template, &resolved)?;

    let mut units = 0;
    let mut longest = 0;
    let mut unicode = false;
    for message in &outgoing {
        let (encoding, parts) = sms::cost(&message.body);
        units += parts;
        longest = longest.max(parts);
        unicode |= encoding == Encoding::Unicode;
    }
    if longest > MAX_PARTS {
        return Err(AppError::validation(format!(
            "This message would take {longest} SMS for each parent. Keep it to {MAX_PARTS} or fewer; anything longer belongs in a printed letter."
        )));
    }

    let mut notes = Vec::new();
    if unicode {
        notes.push(
            "The message has a character outside the standard SMS alphabet (an emoji or a special letter), so each SMS holds 70 characters instead of 160. Removing it would lower the cost."
                .to_string(),
        );
    }
    let shared = resolved.guardians.values().filter(|kids| kids.len() > 1).count();
    if shared > 0 {
        notes.push(format!(
            "{shared} parent{} {} more than one child here and will get one message naming all of them.",
            if shared == 1 { "" } else { "s" },
            if shared == 1 { "has" } else { "have" },
        ));
    }

    let learners = resolved.guardians.values().map(Vec::len).sum::<usize>();
    let mut missing_names: Vec<String> = resolved
        .missing
        .iter()
        .map(|l| format!("{} ({})", l.name, l.class_name))
        .chain(resolved.staff_missing.iter().map(|n| format!("{n} (staff)")))
        .collect();
    let missing_count = missing_names.len();
    missing_names.truncate(MISSING_SHOWN);

    Ok(Plan {
        preview: BroadcastPreview {
            audience_label: audience_label(&resolved.labels),
            recipients: outgoing.len(),
            learners,
            units,
            parts_per_message: longest,
            encoding: if unicode { Encoding::Unicode } else { Encoding::Gsm },
            sample: outgoing.first().map(|m| m.body.clone()),
            missing_count,
            missing_names,
            notes,
        },
        outgoing,
    })
}

/// Who a message will reach and what it will cost. Sends nothing.
#[tauri::command]
pub fn preview_sms_broadcast(
    state: State<'_, AppState>,
    audience: Vec<AudiencePart>,
    message: String,
) -> AppResult<BroadcastPreview> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    Ok(plan(&conn, &session, &audience, &message)?.preview)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastSent {
    pub id: String,
    pub recipients: usize,
    pub units: usize,
    pub audience_label: String,
}

/// Queues the message for everyone in the audience. `expected_units` is the
/// count the sender saw in the preview; if it no longer matches (a learner
/// added, a phone number changed), nothing is sent.
#[tauri::command]
pub fn send_sms_broadcast(
    app: AppHandle,
    state: State<'_, AppState>,
    audience: Vec<AudiencePart>,
    message: String,
    expected_units: usize,
) -> AppResult<BroadcastSent> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let sent = queue_broadcast(&conn, &session, &audience, &message, expected_units)?;
    drop(conn);
    sms::flush_in_background(app);
    Ok(sent)
}

/// Writes the broadcast and every copy to the outbox in one transaction:
/// either every parent's message is queued, or none is.
fn queue_broadcast(
    conn: &Connection,
    session: &Session,
    audience: &[AudiencePart],
    message: &str,
    expected_units: usize,
) -> AppResult<BroadcastSent> {
    let Plan { preview, outgoing } = plan(conn, session, audience, message)?;

    if outgoing.is_empty() {
        return Err(AppError::validation(
            "No one in this audience has a phone number that can receive an SMS.",
        ));
    }
    if preview.units != expected_units {
        return Err(AppError::conflict(format!(
            "The audience changed since the preview: this would now cost {} SMS, not {expected_units}. Check the preview again.",
            preview.units
        )));
    }

    let id = new_id(BROADCAST_PREFIX);
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO sms_broadcasts (id, audience, audience_label, template, recipients, units, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            serde_json::to_string(audience).map_err(|e| AppError::internal(e.to_string()))?,
            preview.audience_label,
            message.trim(),
            outgoing.len() as i64,
            preview.units as i64,
            session.user_id,
            now,
        ],
    )?;
    for message in &outgoing {
        sms::insert(&tx, &message.to, &message.body, "bulk", Some(&id), Some(&session.user_id))?;
    }
    audit::record(
        &tx,
        Some(session),
        "sms.broadcast",
        "sms_broadcast",
        &id,
        format!(
            "Texted {} ({} number{}, {} SMS)",
            preview.audience_label,
            outgoing.len(),
            if outgoing.len() == 1 { "" } else { "s" },
            preview.units
        ),
    )?;
    tx.commit()?;

    Ok(BroadcastSent {
        id,
        recipients: outgoing.len(),
        units: preview.units,
        audience_label: preview.audience_label,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastRow {
    pub id: String,
    pub audience_label: String,
    pub template: String,
    pub recipients: i64,
    pub units: i64,
    pub sent: i64,
    pub waiting: i64,
    pub failed: i64,
    pub sent_by: String,
    pub created_at: String,
}

/// Recent broadcasts and how far each has got. Teachers see their own.
#[tauri::command]
pub fn list_sms_broadcasts(state: State<'_, AppState>) -> AppResult<Vec<BroadcastRow>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT b.id, b.audience_label, b.template, b.recipients, b.units,
                (SELECT COUNT(*) FROM sms_outbox o WHERE o.related_id = b.id AND o.kind = 'bulk' AND o.status = 'sent'),
                (SELECT COUNT(*) FROM sms_outbox o WHERE o.related_id = b.id AND o.kind = 'bulk' AND o.status = 'queued'),
                (SELECT COUNT(*) FROM sms_outbox o WHERE o.related_id = b.id AND o.kind = 'bulk' AND o.status = 'failed'),
                u.full_name, b.created_at
         FROM sms_broadcasts b JOIN users u ON u.id = b.created_by
         WHERE ?1 OR b.created_by = ?2
         ORDER BY b.created_at DESC LIMIT 100",
    )?;
    let rows = stmt
        .query_map(params![session.has(Permission::SendMessages), session.user_id], |r| {
            Ok(BroadcastRow {
                id: r.get(0)?,
                audience_label: r.get(1)?,
                template: r.get(2)?,
                recipients: r.get(3)?,
                units: r.get(4)?,
                sent: r.get(5)?,
                waiting: r.get(6)?,
                failed: r.get(7)?,
                sent_by: r.get(8)?,
                created_at: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Learner groups
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub id: String,
    pub name: String,
    pub note: Option<String>,
    pub leader_id: Option<String>,
    pub leader_name: Option<String>,
    pub members: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    pub class_name: Option<String>,
    pub has_phone: bool,
}

/// Groups this person may see: every group for a School Admin, the groups
/// they lead for a teacher.
#[tauri::command]
pub fn list_learner_groups(state: State<'_, AppState>) -> AppResult<Vec<GroupRow>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.note, g.leader_id, u.full_name,
                (SELECT COUNT(*) FROM learner_group_members m JOIN students s ON s.id = m.student_id
                  WHERE m.group_id = g.id AND m.status = 'active' AND s.status = 'active')
         FROM learner_groups g LEFT JOIN users u ON u.id = g.leader_id
         WHERE g.status = 'active' AND (?1 OR g.leader_id = ?2)
         ORDER BY g.name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![session.has(Permission::SendMessages), session.user_id], |r| {
            Ok(GroupRow {
                id: r.get(0)?,
                name: r.get(1)?,
                note: r.get(2)?,
                leader_id: r.get(3)?,
                leader_name: r.get(4)?,
                members: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn clean(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn check_leader(conn: &Connection, leader_id: &Option<String>) -> AppResult<()> {
    if let Some(id) = clean(leader_id) {
        let ok: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM users WHERE id = ?1 AND status = 'active')",
            params![id],
            |r| r.get(0),
        )?;
        if !ok {
            return Err(AppError::validation("That member of staff is not active."));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInput {
    pub name: String,
    pub note: Option<String>,
    pub leader_id: Option<String>,
}

/// Creating, renaming and retiring groups is office work.
#[tauri::command]
pub fn create_learner_group(state: State<'_, AppState>, input: GroupInput) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require(Permission::SendMessages)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("Give the group a name, such as Football team."));
    }
    let conn = state.db.lock();
    check_leader(&conn, &input.leader_id)?;
    let id = new_id(GROUP_PREFIX);
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO learner_groups (id, name, note, leader_id, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, name, clean(&input.note), clean(&input.leader_id), session.user_id, now],
    )
    .map_err(|e| unique_name(e, name))?;
    audit::record(&tx, Some(&session), "group.create", "learner_group", &id, format!("Created the group {name}"))?;
    tx.commit()?;
    Ok(id)
}

fn unique_name(err: rusqlite::Error, name: &str) -> AppError {
    if let rusqlite::Error::SqliteFailure(e, _) = &err {
        if e.code == rusqlite::ErrorCode::ConstraintViolation {
            return AppError::conflict(format!("There is already a group called {name}."));
        }
    }
    err.into()
}

#[tauri::command]
pub fn update_learner_group(
    state: State<'_, AppState>,
    group_id: String,
    input: GroupInput,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::SendMessages)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("A group needs a name."));
    }
    let conn = state.db.lock();
    check_leader(&conn, &input.leader_id)?;
    let tx = conn.unchecked_transaction()?;
    let changed = tx
        .execute(
            "UPDATE learner_groups SET name = ?1, note = ?2, leader_id = ?3, updated_at = ?4
             WHERE id = ?5 AND status = 'active'",
            params![name, clean(&input.note), clean(&input.leader_id), Utc::now().to_rfc3339(), group_id],
        )
        .map_err(|e| unique_name(e, name))?;
    if changed == 0 {
        return Err(AppError::not_found("group"));
    }
    audit::record(&tx, Some(&session), "group.update", "learner_group", &group_id, format!("Updated the group {name}"))?;
    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn retire_learner_group(state: State<'_, AppState>, group_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::SendMessages)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let name: String = tx
        .query_row(
            "SELECT name FROM learner_groups WHERE id = ?1 AND status = 'active'",
            params![group_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("group"))?;
    tx.execute(
        "UPDATE learner_groups SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![Utc::now().to_rfc3339(), group_id],
    )?;
    audit::record(&tx, Some(&session), "group.retire", "learner_group", &group_id, format!("Retired the group {name}"))?;
    tx.commit()?;
    Ok(())
}

/// The office, or the teacher in charge, keeps the member list.
fn require_group_keeper(conn: &Connection, session: &Session, group_id: &str) -> AppResult<String> {
    let (name, leader): (String, Option<String>) = conn
        .query_row(
            "SELECT name, leader_id FROM learner_groups WHERE id = ?1 AND status = 'active'",
            params![group_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("group"))?;
    if session.has(Permission::SendMessages) || leader.as_deref() == Some(session.user_id.as_str()) {
        Ok(name)
    } else {
        Err(AppError::Forbidden)
    }
}

#[tauri::command]
pub fn list_group_members(state: State<'_, AppState>, group_id: String) -> AppResult<Vec<GroupMember>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_group_keeper(&conn, &session, &group_id)?;
    let year = repo::current_academic_year_id(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, s.reg_number,
                (SELECT c.name FROM enrollments e JOIN classes c ON c.id = e.class_id
                  WHERE e.student_id = s.id AND e.status = 'active'
                    AND (?2 IS NULL OR e.academic_year_id = ?2) LIMIT 1),
                s.guardian_phone
         FROM learner_group_members m JOIN students s ON s.id = m.student_id
         WHERE m.group_id = ?1 AND m.status = 'active' AND s.status = 'active'
         ORDER BY s.full_name",
    )?;
    let rows = stmt
        .query_map(params![group_id, year], |r| {
            let phone: Option<String> = r.get(4)?;
            Ok(GroupMember {
                student_id: r.get(0)?,
                full_name: r.get(1)?,
                reg_number: r.get(2)?,
                class_name: r.get(3)?,
                has_phone: phone.as_deref().and_then(sms::normalize_phone).is_some(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Adds and removes members in one step. Removing switches membership off;
/// it is never deleted.
#[tauri::command]
pub fn change_group_members(
    state: State<'_, AppState>,
    group_id: String,
    add: Vec<String>,
    remove: Vec<String>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let name = require_group_keeper(&conn, &session, &group_id)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;

    let mut added = 0;
    for student_id in &add {
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM students WHERE id = ?1 AND status = 'active')",
            params![student_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(AppError::validation("One of those learners is no longer at the school."));
        }
        added += tx.execute(
            "INSERT INTO learner_group_members (group_id, student_id, status, added_by, added_at)
             VALUES (?1, ?2, 'active', ?3, ?4)
             ON CONFLICT (group_id, student_id) DO UPDATE
                SET status = 'active', added_by = ?3, added_at = ?4, removed_at = NULL
              WHERE learner_group_members.status <> 'active'",
            params![group_id, student_id, session.user_id, now],
        )?;
    }
    let mut removed = 0;
    for student_id in &remove {
        removed += tx.execute(
            "UPDATE learner_group_members SET status = 'removed', removed_at = ?1
             WHERE group_id = ?2 AND student_id = ?3 AND status = 'active'",
            params![now, group_id, student_id],
        )?;
    }
    if added + removed > 0 {
        audit::record(
            &tx,
            Some(&session),
            "group.members",
            "learner_group",
            &group_id,
            format!("{name}: added {added}, removed {removed}"),
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    /// A small school: P3 and P5, last year's leaver, a family with children
    /// in both classes, a learner with no phone, a teacher who is also a
    /// parent, and a football team led by a second teacher.
    fn school() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute_batch(
                "INSERT INTO institution (id, name, created_at, updated_at)
                     VALUES (1, 'Rainbow N&P', 'now', 'now');
                 INSERT INTO academic_years (id, label, status, created_at, updated_at) VALUES
                     ('y25', '2025', 'sealed', 'now', 'now'),
                     ('y26', '2026', 'active', 'now', 'now');
                 INSERT INTO classes (id, code, name, level_kind, ladder_position, created_at, updated_at) VALUES
                     ('p3', 'P3', 'P3', 'primary', 6, 'now', 'now'),
                     ('p5', 'P5', 'P5', 'primary', 8, 'now', 'now');
                 INSERT INTO streams (id, class_id, name, created_at, updated_at) VALUES
                     ('p5e', 'p5', 'East', 'now', 'now');
                 INSERT INTO users (id, username, full_name, phone, role, password_hash, created_at, updated_at) VALUES
                     ('admin',  'admin',  'Office Admin', NULL,          'school_admin', 'x', 'now', 'now'),
                     ('okello', 'okello', 'Mr Okello',    '0772 000 009', 'teacher',     'x', 'now', 'now'),
                     ('coach',  'coach',  'Ms Coach',     '0701 000 001', 'teacher',     'x', 'now', 'now');
                 INSERT INTO students (id, reg_number, full_name, guardian_phone, created_at, updated_at) VALUES
                     ('amina', 'R1', 'Amina N', '0772 000 001',     'now', 'now'),
                     ('musa',  'R2', 'Musa N',  '+256772000001',    'now', 'now'),
                     ('sarah', 'R3', 'Sarah K', '0772000003',       'now', 'now'),
                     ('nophone','R4','Peter Z', 'n/a',              'now', 'now'),
                     ('teachkid','R5','Ivan O', '0772 000 009',     'now', 'now'),
                     ('leaver','R6', 'Old Boy', '0772000006',       'now', 'now');
                 INSERT INTO enrollments (id, student_id, class_id, stream_id, academic_year_id, joined_at, created_at, updated_at) VALUES
                     ('e1', 'amina',   'p3', NULL,  'y26', 'now', 'now', 'now'),
                     ('e2', 'musa',    'p5', 'p5e', 'y26', 'now', 'now', 'now'),
                     ('e3', 'sarah',   'p5', NULL,  'y26', 'now', 'now', 'now'),
                     ('e4', 'nophone', 'p5', NULL,  'y26', 'now', 'now', 'now'),
                     ('e5', 'teachkid','p3', NULL,  'y26', 'now', 'now', 'now'),
                     ('e6', 'leaver',  'p5', NULL,  'y25', 'now', 'now', 'now');
                 INSERT INTO learner_groups (id, name, leader_id, created_at, updated_at) VALUES
                     ('football', 'Football team', 'coach', 'now', 'now');
                 INSERT INTO learner_group_members (group_id, student_id, added_at) VALUES
                     ('football', 'musa', 'now'), ('football', 'sarah', 'now');",
            )
            .unwrap();
        db
    }

    fn person(id: &str, role: Role, class_teacher_of: &[&str]) -> Session {
        test_session(id, role, None, class_teacher_of)
    }

    fn admin() -> Session {
        person("admin", Role::SchoolAdmin, &[])
    }

    fn part(kind: &str, id: Option<&str>) -> AudiencePart {
        AudiencePart { kind: kind.into(), id: id.map(str::to_string) }
    }

    fn preview(db: &Database, who: &Session, audience: &[AudiencePart], text: &str) -> AppResult<BroadcastPreview> {
        plan(&db.lock(), who, audience, text).map(|p| p.preview)
    }

    #[test]
    fn a_parent_of_two_gets_one_text_naming_both() {
        let db = school();
        let plan = plan(&db.lock(), &admin(), &[part("school", None)], "{learner} ({class}): school closes Friday").unwrap();
        let to_family: Vec<&Outgoing> = plan.outgoing.iter().filter(|m| m.to == "+256772000001").collect();
        assert_eq!(to_family.len(), 1, "one message for the shared number");
        assert_eq!(to_family[0].body, "Rainbow N&P: Amina N and Musa N (P3 and P5): school closes Friday");
        assert!(plan.preview.notes.iter().any(|n| n.contains("more than one child")));
    }

    #[test]
    fn last_years_leavers_and_unusable_numbers_are_not_texted() {
        let db = school();
        let p = preview(&db, &admin(), &[part("class", Some("p5"))], "Meeting").unwrap();
        // Musa and Sarah; Peter has no usable number; the leaver is last year's.
        assert_eq!((p.recipients, p.learners), (2, 2));
        assert_eq!(p.missing_count, 1);
        assert_eq!(p.missing_names, vec!["Peter Z (P5)".to_string()]);
    }

    #[test]
    fn overlapping_parts_count_each_learner_once() {
        let db = school();
        let p = preview(
            &db,
            &admin(),
            &[part("class", Some("p5")), part("stream", Some("p5e")), part("group", Some("football"))],
            "Match on Saturday",
        )
        .unwrap();
        assert_eq!(p.recipients, 2);
        assert_eq!(p.audience_label, "P5, P5 East and the Football team group");
    }

    #[test]
    fn a_class_teacher_texts_their_class_and_nothing_wider() {
        let db = school();
        let teacher = person("okello", Role::Teacher, &["p5"]);
        assert!(preview(&db, &teacher, &[part("class", Some("p5"))], "Hi").is_ok());
        assert!(preview(&db, &teacher, &[part("stream", Some("p5e"))], "Hi").is_ok());
        for denied in [part("class", Some("p3")), part("school", None), part("staff", None), part("group", Some("football"))] {
            let result = preview(&db, &teacher, &[part("class", Some("p5")), denied.clone()], "Hi");
            assert!(matches!(result, Err(AppError::Forbidden)), "{denied:?}");
        }
    }

    #[test]
    fn the_teacher_in_charge_texts_their_group() {
        let db = school();
        let coach = person("coach", Role::Teacher, &[]);
        assert_eq!(preview(&db, &coach, &[part("group", Some("football"))], "Training at 4").unwrap().recipients, 2);
        assert!(preview(&db, &coach, &[part("class", Some("p5"))], "Hi").is_err());
    }

    #[test]
    fn the_front_office_texts_anyone_and_the_dos_only_their_own_class() {
        let db = school();
        let secretary = test_session("sec", Role::Teacher, Some(Office::Secretary), &[]);
        assert!(preview(&db, &secretary, &[part("school", None), part("staff", None)], "Closing early").is_ok());
        let dos = test_session("dos", Role::Teacher, Some(Office::DirectorOfStudies), &[]);
        assert!(matches!(preview(&db, &dos, &[part("school", None)], "Hi"), Err(AppError::Forbidden)));
    }

    #[test]
    fn the_school_can_stop_teachers_texting() {
        let db = school();
        repo::set_setting(&db.lock(), "sms.teachers_may_broadcast", "0").unwrap();
        let teacher = person("okello", Role::Teacher, &["p5"]);
        assert!(matches!(
            preview(&db, &teacher, &[part("class", Some("p5"))], "Hi"),
            Err(AppError::Validation(_))
        ));
        assert!(preview(&db, &admin(), &[part("class", Some("p5"))], "Hi").is_ok());
    }

    #[test]
    fn a_teacher_who_is_also_a_parent_gets_the_staff_copy_only() {
        let db = school();
        let plan = plan(&db.lock(), &admin(), &[part("class", Some("p3")), part("staff", None)], "Closing early today").unwrap();
        let to_okello: Vec<&Outgoing> = plan.outgoing.iter().filter(|m| m.to == "+256772000009").collect();
        assert_eq!(to_okello.len(), 1);
        // Office Admin has no phone and is listed as missing.
        assert!(plan.preview.missing_names.iter().any(|n| n == "Office Admin (staff)"));
    }

    #[test]
    fn parent_placeholders_cannot_go_to_staff() {
        let db = school();
        assert!(preview(&db, &admin(), &[part("staff", None)], "Dear {learner}").is_err());
        assert!(preview(&db, &admin(), &[part("class", Some("p3"))], "Dear {name}").is_err());
        let p = preview(&db, &admin(), &[part("staff", None)], "Dear {name}, meeting at 5").unwrap();
        assert!(p.sample.unwrap().contains("Dear "));
    }

    #[test]
    fn pasted_curly_quotes_do_not_double_the_bill() {
        let db = school();
        let p = preview(&db, &admin(), &[part("class", Some("p5"))], "Parents\u{2019} day \u{2013} bring a chair").unwrap();
        assert_eq!(p.encoding, Encoding::Gsm);
        assert_eq!(p.units, 2);
        assert!(p.sample.unwrap().contains("Parents' day - bring a chair"));
    }

    #[test]
    fn an_essay_is_refused() {
        let db = school();
        let essay = "word ".repeat(200);
        assert!(preview(&db, &admin(), &[part("class", Some("p5"))], &essay).is_err());
    }

    #[test]
    fn sending_queues_one_bulk_row_per_number_and_records_it() {
        let db = school();
        let conn = db.lock();
        let units = plan(&conn, &admin(), &[part("school", None)], "Reopening Monday").unwrap().preview.units;
        let sent = queue_broadcast(&conn, &admin(), &[part("school", None)], "Reopening Monday", units).unwrap();
        // Shared family number, Sarah, Okello's child: three numbers.
        assert_eq!(sent.recipients, 3);
        let (rows, priority): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), MIN(priority) FROM sms_outbox WHERE kind = 'bulk' AND related_id = ?1",
                [&sent.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rows, priority), (3, sms::PRIORITY_BULK));
        let audited: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_log WHERE action = 'sms.broadcast'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(audited, 1);
    }

    #[test]
    fn a_stale_preview_sends_nothing() {
        let db = school();
        let conn = db.lock();
        let units = plan(&conn, &admin(), &[part("class", Some("p5"))], "Hi").unwrap().preview.units;
        conn.execute("UPDATE students SET guardian_phone = '0772000004' WHERE id = 'nophone'", []).unwrap();
        let result = queue_broadcast(&conn, &admin(), &[part("class", Some("p5"))], "Hi", units);
        assert!(matches!(result, Err(AppError::Conflict(_))));
        let queued: i64 = conn.query_row("SELECT COUNT(*) FROM sms_outbox", [], |r| r.get(0)).unwrap();
        assert_eq!(queued, 0);
    }

    #[test]
    fn names_read_the_way_a_person_writes_them() {
        let n = |v: &[&str]| join_names(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(n(&[]), "");
        assert_eq!(n(&["Amina"]), "Amina");
        assert_eq!(n(&["Amina", "Musa"]), "Amina and Musa");
        assert_eq!(n(&["Amina", "Musa", "Sarah"]), "Amina, Musa and Sarah");
    }

    #[test]
    fn a_misspelt_placeholder_is_caught_before_800_parents_see_it() {
        assert!(check_placeholders("Dear parent of {leaner}").is_err());
        assert!(check_placeholders("Dear parent of {learner").is_err());
        assert!(check_placeholders("Dear parent of {Learner} in { class }").is_ok());
        assert!(check_placeholders("No placeholders at all").unwrap().is_empty());
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            fill("{learner} in {class} has a match", "Amina and Musa", "P3 and P5", ""),
            "Amina and Musa in P3 and P5 has a match"
        );
        assert_eq!(fill("Dear {name}, staff meeting", "", "", "Mr Okello"), "Dear Mr Okello, staff meeting");
    }
}
