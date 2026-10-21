//! The owner's tool for Termbook licence keys.
//!
//! ```text
//! termbook-licence keygen                 make your signing key (once, ever)
//! termbook-licence issue  --school CODE --name "School" --plan term --from 2026-09-01 [--until 2026-12-12] [--id R-0001]
//! termbook-licence inspect TBK1....        read a key and check it
//! termbook-licence pubkey                  print the public key (safe to share)
//! ```
//!
//! The signing key (`termbook-issuer.key`) is the only thing that can make a
//! key a school's copy of Termbook will accept. Keep it off any school
//! computer, back it up somewhere safe and offline, and never send it to
//! anyone. Lose it and every school needs a new copy of Termbook with a new
//! public key; leak it and anyone can issue keys.

#[allow(dead_code)]
#[path = "../../../src-tauri/src/licence_format.rs"]
mod licence_format;

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use chrono::{Duration, Local, NaiveDate};
use ed25519_dalek::{Signer, SigningKey};
use licence_format::{decode, encode, Claims};

const KEY_FILE: &str = "termbook-issuer.key";
const REGISTER: &str = "issued-keys.csv";
/// FR-W2: a term is about 120 days, a year 365.
const TERM_DAYS: i64 = 120;
const YEAR_DAYS: i64 = 365;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(&args[1..]),
        Some("issue") => issue(&args[1..]),
        Some("inspect") => inspect(&args[1..]),
        Some("pubkey") => pubkey(&args[1..]),
        _ => Err(USAGE.to_string()),
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

const USAGE: &str = "Usage:
  termbook-licence keygen [--key FILE]
  termbook-licence issue --school CODE --name NAME --plan term|year --from YYYY-MM-DD [--until YYYY-MM-DD] [--id REF] [--key FILE]
  termbook-licence inspect KEY [--key FILE]
  termbook-licence pubkey [--key FILE]";

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn key_path(args: &[String]) -> PathBuf {
    PathBuf::from(flag(args, "--key").unwrap_or_else(|| KEY_FILE.to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn load(path: &PathBuf) -> Result<SigningKey, String> {
    let text = fs::read_to_string(path)
        .map_err(|_| format!("No signing key at {}. Run `termbook-licence keygen` first.", path.display()))?;
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(text.trim().get(i..i + 2)?, 16).ok())
        .collect();
    let seed: [u8; 32] = bytes.try_into().map_err(|_| format!("{} is not a signing key.", path.display()))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Prints only the public half, for owner-setup to put into the application.
/// The public key is safe to share; the signing key never leaves its file.
fn pubkey(args: &[String]) -> Result<(), String> {
    let signing = load(&key_path(args))?;
    println!("{}", hex(signing.verifying_key().as_bytes()));
    Ok(())
}

fn keygen(args: &[String]) -> Result<(), String> {
    let path = key_path(args);
    if path.exists() {
        return Err(format!(
            "{} already exists. Making a new key would invalidate every key you have issued. \
             If you really mean it, move the old file somewhere safe first.",
            path.display()
        ));
    }
    let signing = SigningKey::generate(&mut rand_core::OsRng);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    writeln!(file, "{}", hex(&signing.to_bytes())).map_err(|e| e.to_string())?;
    println!("Signing key written to {}.", path.display());
    println!("Back it up offline now (a USB stick in a safe place). Never put it on a school computer.\n");
    println!("Public key, for src-tauri/src/licence.rs:\n");
    println!("pub const ISSUER_PUBLIC_KEY_HEX: &str = \"{}\";", hex(signing.verifying_key().as_bytes()));
    Ok(())
}

fn issue(args: &[String]) -> Result<(), String> {
    let signing = load(&key_path(args))?;
    let need = |name: &str| flag(args, name).ok_or_else(|| format!("Missing {name}.\n\n{USAGE}"));
    let school = need("--school")?.trim().to_uppercase().replace('-', "");
    let name = need("--name")?;
    let plan = need("--plan")?;
    let days = match plan.as_str() {
        "term" => TERM_DAYS,
        "year" => YEAR_DAYS,
        _ => return Err("--plan is term or year.".into()),
    };
    let date = |text: String| NaiveDate::parse_from_str(&text, "%Y-%m-%d").map_err(|_| format!("{text} is not a date like 2026-09-01."));
    let from = date(need("--from")?)?;
    let until = match flag(args, "--until") {
        Some(text) => date(text)?,
        None => from + Duration::days(days - 1),
    };
    let id = flag(args, "--id").unwrap_or_else(|| format!("R-{}", Local::now().format("%Y%m%d-%H%M%S")));
    let claims = Claims { v: 1, school, name, plan, from, until, id };
    let key = encode(&claims, |body| signing.sign(body).to_bytes());

    // Check it the way the application will before handing it over.
    decode(&key, &signing.verifying_key()).map_err(|e| format!("The key did not check: {e}"))?;

    let new_register = !PathBuf::from(REGISTER).exists();
    let mut register = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(REGISTER)
        .map_err(|e| format!("Could not write {REGISTER}: {e}"))?;
    if new_register {
        writeln!(register, "issued_at,id,school_code,school_name,plan,from,until").map_err(|e| e.to_string())?;
    }
    writeln!(
        register,
        "{},{},{},\"{}\",{},{},{}",
        Local::now().format("%Y-%m-%d %H:%M"),
        claims.id,
        claims.school,
        claims.name.replace('"', "'"),
        claims.plan,
        claims.from,
        claims.until
    )
    .map_err(|e| e.to_string())?;

    println!("{} — {} plan, {} to {} (ref {})\n", claims.name, claims.plan, claims.from, claims.until, claims.id);
    println!("{key}\n");
    println!("Send the line above to the school. They paste it into Settings → Subscription.");
    println!("Recorded in {REGISTER}.");
    Ok(())
}

fn inspect(args: &[String]) -> Result<(), String> {
    let key = args.first().ok_or_else(|| USAGE.to_string())?;
    let signing = load(&key_path(args))?;
    let claims = decode(key, &signing.verifying_key()).map_err(str::to_string)?;
    println!("{}", serde_json::to_string_pretty(&claims).unwrap());
    println!("\nSigned by your key: yes.");
    Ok(())
}
