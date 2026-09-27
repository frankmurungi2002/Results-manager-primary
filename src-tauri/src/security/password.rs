//! Password hashing and the rules around choosing one.
//!
//! Argon2id with the crate defaults (19 MiB, 2 passes, 1 lane), which is the
//! current OWASP recommendation. Nothing here ever stores or logs a plaintext
//! password; callers hand over a `Secret` that wipes itself on drop.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::Rng;
use zeroize::Zeroize;

use crate::error::{AppError, AppResult};

/// A plaintext password that zeroes its memory when dropped.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <String as serde::Deserialize>::deserialize(deserializer).map(Secret)
    }
}

/// Hashes a password for storage. The returned string is a full PHC string, so
/// it carries its own salt and parameters and can be re-verified after a
/// parameter change.
pub fn hash(password: &Secret) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.expose().as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::internal(format!("password hashing failed: {e}")))
}

/// Constant-time-ish verification. A malformed stored hash is treated as a
/// failed match, never as a panic.
pub fn verify(password: &Secret, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        log::error!("stored password hash is malformed");
        return false;
    };

    Argon2::default()
        .verify_password(password.expose().as_bytes(), &parsed)
        .is_ok()
}

/// Minimum bar for a password a school actually has to remember and type.
/// Deliberately length-first rather than a character-class puzzle, which is
/// both weaker in practice and harder for a headteacher to comply with.
pub fn check_strength(password: &Secret) -> AppResult<()> {
    let value = password.expose();
    let length = value.chars().count();

    if length < 10 {
        return Err(AppError::validation(
            "A password must be at least 10 characters long.",
        ));
    }
    if length > 256 {
        return Err(AppError::validation("That password is too long."));
    }

    let lower = value.to_lowercase();
    const OBVIOUS: &[&str] = &[
        "password",
        "12345678",
        "qwerty",
        "letmein",
        "admin123",
        "resultsmanager",
        "schooladmin",
    ];
    if OBVIOUS.iter().any(|bad| lower.contains(bad)) {
        return Err(AppError::validation(
            "That password is too easy to guess. Please choose another.",
        ));
    }

    // Reject a single repeated character ("aaaaaaaaaa").
    if value.chars().collect::<std::collections::HashSet<_>>().len() < 4 {
        return Err(AppError::validation(
            "That password repeats too few different characters.",
        ));
    }

    Ok(())
}

/// Generates an initial password a School Admin can read off a screen and type
/// without ambiguity: no 0/O, no 1/l/I, grouped for legibility.
pub fn generate_initial() -> Secret {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();

    let mut out = String::with_capacity(14);
    for group in 0..3 {
        if group > 0 {
            out.push('-');
        }
        for _ in 0..4 {
            let idx = rng.gen_range(0..ALPHABET.len());
            out.push(ALPHABET[idx] as char);
        }
    }
    Secret(out)
}

/// The generated password, revealed once so it can be shown to the person it
/// belongs to. Nothing else in the codebase may read a `Secret`.
pub fn reveal(secret: &Secret) -> String {
    secret.expose().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_round_trips() {
        let password = Secret::new("a-quite-long-passphrase");
        let stored = hash(&password).expect("hash");
        assert!(verify(&password, &stored));
        assert!(!verify(&Secret::new("a-quite-long-passphrasE"), &stored));
    }

    #[test]
    fn two_hashes_of_the_same_password_differ() {
        let password = Secret::new("a-quite-long-passphrase");
        assert_ne!(hash(&password).unwrap(), hash(&password).unwrap());
    }

    #[test]
    fn malformed_hash_fails_closed() {
        assert!(!verify(&Secret::new("anything at all"), "not-a-phc-string"));
    }

    #[test]
    fn strength_rules_bite() {
        assert!(check_strength(&Secret::new("short")).is_err());
        assert!(check_strength(&Secret::new("password123")).is_err());
        assert!(check_strength(&Secret::new("aaaaaaaaaaaa")).is_err());
        assert!(check_strength(&Secret::new("Kampala-Rain-2026")).is_ok());
    }

    #[test]
    fn generated_passwords_pass_their_own_rules() {
        for _ in 0..50 {
            let generated = generate_initial();
            assert!(check_strength(&generated).is_ok(), "{generated:?}");
        }
    }
}
