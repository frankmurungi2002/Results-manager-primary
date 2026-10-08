# Subscriptions and licence keys (FR-B11)

How a school pays, termly or yearly, and how Phantom School Manager knows,
with no internet.

## For the school

1. Settings → Subscription shows the **school code**, e.g. `9F3AC-2B1E0`.
2. The school pays for a **term** (about 120 days) or a **year** (365) and
   sends the school code with the payment reference.
3. It receives a **licence key**, one long line starting `PSM1.`, and pastes
   it into Settings → Subscription → Install key.

A new school has a **30-day trial** from the day setup was completed.

When a subscription ends, Phantom keeps working fully for **7 days of grace**,
with a warning on every screen. After that it becomes **read-only**:
everything can still be viewed, printed and backed up, and texts already
queued still go out, but nothing can be added or changed until a new key is
entered. A school is never locked out of its own records.

## For the owner (you)

### Once, ever: make your signing key

```bash
cd tools/phantom-licence
cargo run --release -- keygen
```

This writes `phantom-issuer.key` and prints a line like:

```rust
pub const ISSUER_PUBLIC_KEY_HEX: &str = "3b6a27bc...";
```

Paste that line over the empty one in `src-tauri/src/licence.rs` and build.
From that build on, subscriptions are switched on. Until then every copy runs
as a pilot with no limits, which is what Rainbow is on now.

**The signing key is the business.** Anyone who has it can issue keys for
every school. Keep it on your own computer, never on a school's; back it up
offline (a USB stick somewhere safe); never email or WhatsApp it. It is
listed in `.gitignore` so it cannot be committed by accident. If you lose
it, every school needs a new copy of Phantom with a new public key.

### Each payment: issue a key

```bash
cargo run --release -- issue --school 9F3AC-2B1E0 --name "Rainbow Nursery and Primary School" \
    --plan term --from 2026-09-01 --id R-0042
```

`--until` is optional: a term runs 120 days from `--from`, a year 365. Set it
to match the school calendar if you prefer. The key is printed; send that one
line to the school. Every key you issue is also recorded in
`issued-keys.csv`, your sales register.

`inspect PSM1...` reads any key back and confirms you signed it.

## What it protects, honestly

- A key **cannot be forged** without your signing key (Ed25519 signatures).
- A key **cannot be edited**: changing its date breaks the signature.
- A key **works only for the school it was issued to**: its code is inside.
- Turning the computer's **clock back** does not help: Phantom remembers the
  latest date it has seen and uses that.
- Read-only is enforced **inside SQLite** (an authorizer on the connection),
  so no screen or command can forget to check it.

What it is not: protection against someone who patches the program itself,
or who restores an old database *and* turns the clock back. It keeps honest
schools honest and makes a lapsed subscription obvious, which is what
manual activation needs (SRS 16.4). Online payment and automatic keys come
with the website (FR-W4, FR-W6); the tool's `issue` is what the website will
call.
