//! The licence key format, shared word for word by the application (which
//! only verifies) and `tools/termbook-licence` (which signs). Both compile this
//! one file, so a key the tool issues is always one the application reads.
//! Nothing here may depend on the rest of the application.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::NaiveDate;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "TBK1";

/// What the owner signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Format version.
    pub v: u8,
    /// The school code this key is for.
    pub school: String,
    /// The school's name as the owner recorded it, shown back for checking.
    pub name: String,
    /// `term` or `year`.
    pub plan: String,
    /// First and last day covered, inclusive (YYYY-MM-DD).
    pub from: NaiveDate,
    pub until: NaiveDate,
    /// The owner's reference for this key, e.g. a receipt number.
    pub id: String,
}

/// Reads a key and checks its signature. Errors are sentences for a person.
pub fn decode(key: &str, issuer: &VerifyingKey) -> Result<Claims, &'static str> {
    let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();
    let mut parts = compact.split('.');
    let (Some(prefix), Some(body), Some(sig), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err("That is not a Termbook licence key. Paste the whole key, starting TBK1.");
    };
    if prefix != PREFIX {
        return Err("That is not a Termbook licence key. It should start TBK1.");
    }
    let body = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| "Part of that key is missing or mistyped. Paste it again.")?;
    let sig = URL_SAFE_NO_PAD
        .decode(sig)
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
        .ok_or("Part of that key is missing or mistyped. Paste it again.")?;
    issuer
        .verify_strict(&body, &sig)
        .map_err(|_| "That key was not issued by Termbook, or it has been changed.")?;
    let claims: Claims =
        serde_json::from_slice(&body).map_err(|_| "That key is in a format this version cannot read. Update Termbook.")?;
    if claims.v != 1 {
        return Err("That key is for a newer version of Termbook. Update Termbook.");
    }
    if claims.until < claims.from {
        return Err("That key ends before it starts.");
    }
    Ok(claims)
}

/// Builds a key from claims and a signing function.
pub fn encode(claims: &Claims, sign: impl FnOnce(&[u8]) -> [u8; 64]) -> String {
    let body = serde_json::to_vec(claims).expect("claims serialise");
    let sig = sign(&body);
    format!("{PREFIX}.{}.{}", URL_SAFE_NO_PAD.encode(&body), URL_SAFE_NO_PAD.encode(sig))
}

/// Parses 64 hex characters into a public key.
pub fn public_key_from_hex(hex: &str) -> Option<VerifyingKey> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    let array: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&array).ok()
}
