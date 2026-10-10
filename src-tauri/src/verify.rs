//! Proving a printed receipt or report card is genuine.
//!
//! A forged report card (a better division, a higher position) or a forged
//! fees receipt (a bigger amount) is a real problem for Ugandan schools: the
//! paper looks right and nobody can tell. Each document Phantom prints now
//! carries a short code, and a QR code that holds the facts and that code.
//!
//! * Anyone can scan the QR with an ordinary phone camera and see what the
//!   school actually recorded. If the paper says UGX 700,000 and the QR says
//!   UGX 70,000, the paper was altered.
//! * At the school, "Check a document" in Phantom confirms the code. The code
//!   is an HMAC of the facts under a secret that never leaves this computer,
//!   so nobody can make up a code for facts the school never printed. It
//!   works with no internet.
//!
//! The secret is made once per installation and travels with backups, so a
//! restored school still recognises its own documents.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::repo;
use crate::error::{AppError, AppResult};

const SECRET_KEY: &str = "verify.secret";

/// Crockford's Base32, as the audit log's codes use: no I, L, O or U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn secret(conn: &Connection) -> AppResult<Vec<u8>> {
    if let Some(hex) = repo::get_setting(conn, SECRET_KEY)? {
        if let Some(bytes) = from_hex(&hex) {
            if bytes.len() == 32 {
                return Ok(bytes);
            }
        }
    }
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    repo::set_setting(conn, SECRET_KEY, &to_hex(&bytes))?;
    Ok(bytes.to_vec())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// HMAC-SHA256 (RFC 2104).
fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// "R-7K2M-9QXA-4FTB": a letter for the kind of document, then 60 bits.
fn code(conn: &Connection, prefix: char, facts: &str) -> AppResult<String> {
    let mac = hmac(&secret(conn)?, format!("{prefix}|{facts}").as_bytes());
    let mut bits: u64 = 0;
    for byte in &mac[..8] {
        bits = (bits << 8) | *byte as u64;
    }
    bits >>= 4;
    let body: String = (0..12).rev().map(|i| ALPHABET[((bits >> (i * 5)) & 31) as usize] as char).collect();
    Ok(format!("{prefix}-{}-{}-{}", &body[0..4], &body[4..8], &body[8..12]))
}

/// Upper-cases a code typed from paper and forgives the usual misreadings.
fn normalise(code: &str) -> String {
    code.trim()
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Receipts
// ---------------------------------------------------------------------------

fn receipt_facts(receipt_no: i64, student_id: &str, amount: i64, paid_on: &str) -> String {
    format!("{receipt_no}|{student_id}|{amount}|{paid_on}")
}

pub fn code_for_receipt(conn: &Connection, payment: &crate::commands::fees::PaymentRow) -> AppResult<String> {
    code(conn, 'R', &receipt_facts(payment.receipt_no, &payment.student_id, payment.amount, &payment.paid_on))
}

// ---------------------------------------------------------------------------
// Report cards
// ---------------------------------------------------------------------------

/// What a report card's code vouches for: who, which term, and the results
/// a forger would change.
pub fn report_card_facts(
    student_id: &str,
    term_id: &str,
    total_points: Option<f64>,
    division: Option<&str>,
    position: Option<i64>,
    mean_percentage: Option<f64>,
) -> String {
    format!(
        "{student_id}|{term_id}|{}|{}|{}|{}",
        total_points.map(|v| format!("{v:.1}")).unwrap_or_default(),
        division.unwrap_or(""),
        position.map(|v| v.to_string()).unwrap_or_default(),
        mean_percentage.map(|v| format!("{v:.1}")).unwrap_or_default(),
    )
}

pub fn code_for_report_card(conn: &Connection, facts: &str) -> AppResult<String> {
    code(conn, 'C', facts)
}

// ---------------------------------------------------------------------------
// Checking a document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCheck {
    /// The code is one this school issued for these facts.
    pub genuine: bool,
    /// `receipt` or `report_card`; empty when the code was not recognised.
    pub kind: String,
    pub headline: String,
    /// What the school recorded, to compare with the paper line by line.
    pub facts: Vec<(String, String)>,
    pub warning: Option<String>,
}

/// Checks a receipt from its number and code.
pub fn check_receipt(conn: &Connection, receipt_no: i64, typed: &str) -> AppResult<DocumentCheck> {
    let row: Option<(String, String, String, i64, String, String, String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT p.id, p.student_id, s.full_name, p.amount, p.paid_on, p.method, p.status, p.void_reason,
                    u.full_name
             FROM fee_payments p JOIN students s ON s.id = p.student_id
             LEFT JOIN users u ON u.id = p.received_by
             WHERE p.receipt_no = ?1",
            params![receipt_no],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .optional()?;
    let label = crate::fees::receipt_label(receipt_no);
    let Some((_, student_id, name, amount, paid_on, method, status, void_reason, received_by)) = row else {
        return Ok(DocumentCheck {
            genuine: false,
            kind: "receipt".into(),
            headline: format!("This school never issued receipt {label}."),
            facts: Vec::new(),
            warning: Some("Treat the paper as forged until the bursar can explain it.".into()),
        });
    };
    let expected = code(conn, 'R', &receipt_facts(receipt_no, &student_id, amount, &paid_on))?;
    let genuine = normalise(&expected) == normalise(typed);
    let facts = vec![
        ("Receipt".to_string(), label.clone()),
        ("Learner".to_string(), name),
        ("Amount".to_string(), crate::fees::ugx(amount)),
        ("Paid on".to_string(), paid_on),
        ("Paid by".to_string(), crate::fees::method_label(&method).to_string()),
        ("Received by".to_string(), received_by.unwrap_or_else(|| "—".into())),
    ];
    let warning = if status == "voided" {
        Some(format!(
            "This receipt was CANCELLED{}. It is not proof of payment.",
            void_reason.map(|r| format!(": {r}")).unwrap_or_default()
        ))
    } else if !genuine {
        Some("The code does not match what the school recorded. Compare every line below with the paper.".into())
    } else {
        None
    };
    Ok(DocumentCheck {
        genuine: genuine && status != "voided",
        kind: "receipt".into(),
        headline: if genuine {
            format!("{label} is a genuine receipt from this school.")
        } else {
            format!("The code on {label} is NOT one this school issued.")
        },
        facts,
        warning,
    })
}

/// Checks a report card from the facts a QR carries (or the code alone and
/// the learner's registration number and term).
pub fn check_report_card(conn: &Connection, reg_number: &str, term_id: &str, typed: &str, facts: &str) -> AppResult<DocumentCheck> {
    let expected = code(conn, 'C', facts)?;
    let genuine = normalise(&expected) == normalise(typed);
    let name: Option<String> = conn
        .query_row("SELECT full_name FROM students WHERE reg_number = ?1", params![reg_number], |r| r.get(0))
        .optional()?;
    let term: Option<String> = conn
        .query_row(
            "SELECT t.name || ' ' || y.label FROM terms t JOIN academic_years y ON y.id = t.academic_year_id WHERE t.id = ?1",
            params![term_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(DocumentCheck {
        genuine,
        kind: "report_card".into(),
        headline: if genuine {
            "This report card is genuine: the results match what the school printed.".into()
        } else {
            "This report card does NOT match anything the school printed.".into()
        },
        facts: vec![
            ("Learner".into(), name.unwrap_or_else(|| reg_number.into())),
            ("Term".into(), term.unwrap_or_else(|| term_id.into())),
        ],
        warning: (!genuine).then(|| "The results on the paper may have been changed.".into()),
    })
}

/// What "Check a document" accepts: a code typed from paper with the receipt
/// number, or the text a phone read from the QR.
pub fn check_text(conn: &Connection, text: &str) -> AppResult<DocumentCheck> {
    // The QR holds a few readable lines and, last, the machine line.
    let text = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("PSM|"))
        .unwrap_or(text.trim());
    // QR text: "PSM|R|<receipt_no>|<code>" or "PSM|C|<reg>|<term_id>|<facts...>|<code>".
    if let Some(rest) = text.strip_prefix("PSM|") {
        let parts: Vec<&str> = rest.split('|').collect();
        return match parts.first() {
            Some(&"R") if parts.len() == 3 => {
                let number = parts[1].parse().map_err(|_| AppError::validation("That QR is not readable."))?;
                check_receipt(conn, number, parts[2])
            }
            Some(&"C") if parts.len() >= 4 => {
                let typed = parts[parts.len() - 1];
                let reg = parts[1];
                let facts = parts[2..parts.len() - 1].join("|");
                let term = parts.get(3).copied().unwrap_or("");
                check_report_card(conn, reg, term, typed, &facts)
            }
            _ => Err(AppError::validation("That QR is not a Phantom document.")),
        };
    }
    Err(AppError::validation("Scan the QR code, or enter the receipt number and the code printed beside it."))
}

/// The text a document's QR code holds: readable on any phone, and checkable
/// in Phantom.
///
/// A phone camera shows the readable lines; Phantom reads the last one.
pub fn receipt_qr(school: &str, payment: &crate::commands::fees::PaymentRow, verification_code: &str) -> String {
    format!(
        "{school}\nReceipt {} of {}\n{}\n{} by {}\nCode {verification_code}\nPSM|R|{}|{verification_code}",
        payment.receipt_label,
        payment.paid_on,
        payment.student_name,
        crate::fees::ugx(payment.amount),
        payment.method_label,
        payment.receipt_no,
    )
}

/// `summary` is what the card says, in words: "Term 3 2026: Aggregate 14, Division II, position 9 of 42".
pub fn report_card_qr(school: &str, name: &str, summary: &str, reg_number: &str, facts: &str, verification_code: &str) -> String {
    format!("{school}\n{name} ({reg_number})\n{summary}\nCode {verification_code}\nPSM|C|{reg_number}|{facts}|{verification_code}")
}

/// A QR code as an SVG data URL, drawn in Rust so the page needs no library
/// and loads nothing from anywhere. Medium error correction survives a fold,
/// a smudge or a cheap printer.
pub fn qr_data_url(text: &str) -> AppResult<String> {
    use base64::Engine;
    use qrcodegen::{QrCode, QrCodeEcc};
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium)
        .map_err(|_| AppError::internal("the QR code text is too long"))?;
    let size = qr.size();
    let border = 2;
    let mut path = String::new();
    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x, y) {
                path.push_str(&format!("M{},{}h1v1h-1z", x + border, y + border));
            }
        }
    }
    let full = size + border * 2;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {full} {full}\" shape-rendering=\"crispEdges\">\
         <rect width=\"100%\" height=\"100%\" fill=\"#fff\"/><path fill=\"#000\" d=\"{path}\"/></svg>"
    );
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::fees::{record_payment_in, RecordPaymentRequest};
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    #[test]
    fn hmac_matches_the_rfc_4231_test_vector() {
        let key = [0x0bu8; 20];
        let mac = hmac(&key, b"Hi There");
        assert_eq!(to_hex(&mac), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
    }

    #[test]
    fn a_genuine_receipt_passes_and_an_altered_one_does_not() {
        let db = fee_school();
        let conn = db.lock();
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let request = RecordPaymentRequest {
            student_id: "amina".into(),
            amount: 70_000,
            method: "cash".into(),
            reference: None,
            paid_by: None,
            payer_phone: None,
            paid_on: Some("2026-06-01".into()),
            note: None,
            text_guardian: false,
        };
        let recorded = record_payment_in(&conn, &bursar, &request, "manual").unwrap();
        let printed = code_for_receipt(&conn, &recorded.payment).unwrap();
        assert!(printed.starts_with("R-") && printed.len() == 16, "{printed}");

        let ok = check_receipt(&conn, recorded.payment.receipt_no, &printed.to_lowercase()).unwrap();
        assert!(ok.genuine, "{}", ok.headline);
        assert!(ok.facts.iter().any(|(k, v)| k == "Amount" && v == "UGX 70,000"));

        // The same code presented with another receipt number is refused.
        let other = check_receipt(&conn, 1, &printed).unwrap();
        assert!(!other.genuine);
        // A receipt number the school never reached.
        assert!(check_receipt(&conn, 999, &printed).unwrap().headline.contains("never issued"));
        // Through the QR text too, readable lines and all.
        let qr = receipt_qr("Rainbow N&P", &recorded.payment, &printed);
        assert!(qr.starts_with("Rainbow N&P\nReceipt RCT-000004 of 2026-06-01\nAmina N\nUGX 70,000 by Cash\n"), "{qr}");
        let via_qr = check_text(&conn, &qr).unwrap();
        assert!(via_qr.genuine);
    }

    #[test]
    fn a_cancelled_receipt_is_never_proof_of_payment() {
        let db = fee_school();
        let conn = db.lock();
        conn.execute("UPDATE fee_payments SET status = 'voided', voided_at = 'x', void_reason = 'Bounced cheque' WHERE receipt_no = 3", [])
            .unwrap();
        let payment = crate::commands::fees::load_payment(&conn, "p3").unwrap();
        let printed = code_for_receipt(&conn, &payment).unwrap();
        let check = check_receipt(&conn, 3, &printed).unwrap();
        assert!(!check.genuine);
        assert!(check.warning.unwrap().contains("CANCELLED: Bounced cheque"));
    }

    #[test]
    fn a_changed_division_breaks_a_report_cards_code() {
        let db = fee_school();
        let conn = db.lock();
        let facts = report_card_facts("amina", "t262", Some(14.0), Some("II"), Some(9), Some(68.25));
        let printed = code_for_report_card(&conn, &facts).unwrap();
        let qr = report_card_qr("Rainbow", "Amina N", "Term 2 2026", "R1", &facts, &printed);
        assert!(check_text(&conn, &qr).unwrap().genuine);
        let forged = report_card_qr(
            "Rainbow",
            "Amina N",
            "Term 2 2026",
            "R1",
            &report_card_facts("amina", "t262", Some(6.0), Some("I"), Some(1), Some(88.0)),
            &printed,
        );
        assert!(!check_text(&conn, &forged).unwrap().genuine);
    }

    #[test]
    fn a_qr_is_an_svg_data_url() {
        let url = qr_data_url("PSM|R|4|R-0000-0000-0000").unwrap();
        assert!(url.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn the_secret_survives_and_codes_are_stable() {
        let db = fee_school();
        let conn = db.lock();
        let a = code(&conn, 'R', "1|x|5|2026-01-01").unwrap();
        let b = code(&conn, 'R', "1|x|5|2026-01-01").unwrap();
        assert_eq!(a, b);
        assert_eq!(normalise("r-0lo1-abcd"), "R0101ABCD");
    }
}
