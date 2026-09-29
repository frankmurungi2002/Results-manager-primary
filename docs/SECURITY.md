# Security model

Phantom School Manager holds a school's entire academic history on one PC in a shared
office, run by people who are not IT staff, on a machine that may be handled by
anyone who walks past it. The model below follows from that, not from a generic
checklist.

## The shape of the thing

The webview renders; the Rust backend decides. There is no SQL, no filesystem
access, no network call and no credential in the interface layer. Everything it
can do is a named command, and every command that touches school data resolves
the actor from Rust process memory rather than trusting an argument.

That means a compromised or rewritten webview — a malicious extension, a
tampered bundle, someone with DevTools — cannot claim to be a School Admin,
cannot read a table it was not given, and cannot reach a file. The worst it can
do is call commands as whoever is actually signed in.

### What the webview is allowed

`src-tauri/capabilities/default.json` grants exactly two things: Tauri's core
defaults and the file dialogs. No filesystem plugin, no shell, no HTTP, no
opener. The Content Security Policy in `tauri.conf.json` allows scripts only
from the bundle itself, blocks all framing, and permits no outbound connection
except the IPC channel.

## Passwords

Argon2id at the crate defaults — 19 MiB, 2 passes, 1 lane, which is the current
OWASP recommendation. Each hash carries its own salt and parameters as a PHC
string, so parameters can be raised later without invalidating stored hashes.

- Plaintext passwords live in a `Secret` that zeroes its memory on drop, is
  never logged, and has no `Debug` output.
- A sign-in against an unknown username still runs a verification against a
  dummy hash, so timing does not reveal which accounts exist.
- Five consecutive failures lock an account for fifteen minutes.
- A generated initial password is shown exactly once. RM stores only the hash;
  nobody, including the developer, can recover it.
- Strength is length-first (ten characters minimum) rather than a
  character-class puzzle — the latter is weaker in practice and harder for a
  headteacher to comply with.

## Sessions

The session is held in Rust, not in the webview. There is no token in
`localStorage` to steal, because there is no token.

It expires after thirty minutes of inactivity, enforced at a single choke point
(`SessionStore::require`) that every protected command passes through, so the
rule is applied once rather than remembered in eighty places. A school PC sits
on a desk in a shared office; an unattended signed-in session is a real
exposure.

## Authorisation

Three levels, all decided in Rust:

1. **Role** — School Admin or Teacher.
2. **Class scope (FR-C1)** — a teacher may only see and act on the classes
   they were assigned. A School Admin short-circuits this; a teacher's set is
   loaded from the assignment table at sign-in.
3. **Subject scope** — a Subject Teacher may only write marks for the subject
   they were assigned. Being in the class is not enough.

The caps in FR-B6 (exactly one Class Teacher, at most one Assistant per class)
are unique indexes in the schema, not application checks, so two School Admins
acting at the same moment cannot produce two Class Teachers.

## The audit trail

`audit_log` rejects every `UPDATE` and `DELETE` through database triggers. A
record of what happened that can be edited is not worth keeping.

Entries are written on the same connection and inside the same transaction as
the action they describe, so an action and its audit entry commit together or
not at all. The actor is stored by name as well as by id, so the trail stays
readable after an account is retired and its holder has left the school.

Nothing is ever hard-deleted — learners are dropped, staff and subjects are
retired, enrollments are closed. History always resolves.

## Data at rest

By default the database is plain SQLite, protected by Windows file permissions
in the user's `%APPDATA%`. The learner data never leaves that folder.

### Turning on full-database encryption

```bash
cargo build --release --features encrypted-db
```

This swaps bundled SQLite for SQLCipher (AES-256). A 256-bit key is generated on
first run and stored in the **Windows Credential Manager**, never on the SSDs
beside the data — so a stolen drive is not a readable drive.

It is off by default only because the build needs two extra tools, since
`openssl-src` compiles OpenSSL from source:

- [NASM](https://www.nasm.us/) on `PATH`
- [Strawberry Perl](https://strawberryperl.com/) on `PATH`

Turn it on before a school's first real term. Migrating an existing plain
database means exporting and re-importing, or a `sqlcipher_export` pass — far
easier to do on day one than after a year of records.

## Durability

Power cuts are routine in Uganda, and a half-written mark is worse than a
missing one.

- `journal_mode = WAL` survives an abrupt cut far better than the rollback
  journal.
- `synchronous = FULL`, not `NORMAL`: the fsync costs less than explaining a
  lost mark.
- `foreign_keys = ON`, because the never-delete rules depend on it.

Backups use SQLite's online backup API, which produces a consistent snapshot
while the application is in use. The twenty most recent are kept locally; if a
second drive is configured, each one is written there too. A missing second
drive is reported as a warning rather than failing the backup that already
succeeded locally.

## Input handling

- Every mark is validated against its subject's maximum, in Rust, before
  storage. Rows that fail are reported back individually rather than aborting
  the batch — a teacher who mistypes one mark should not lose the other
  thirty-nine.
- Images are checked for a real PNG signature and a 2 MB ceiling, not trusted by
  extension.
- Every query uses bound parameters. The one string interpolation in the
  codebase is the SQLCipher `PRAGMA key`, which takes hex RM generated itself
  from its own CSPRNG.
- Spreadsheets are parsed in Rust (`calamine`), not in the webview, so an
  untrusted file is handled in a memory-safe sandbox.

## What is deliberately not here yet

Per SRS 16.4, the pilot has **no hardware fingerprint and no payment gateway**.
Activation is manual. Both add support burden and lock-out risk before there is
volume to justify them, and both are hard to test properly before real
transactions exist. They belong in Phase 2.

Cloud backup (FR-W8) is likewise not built. When it is, snapshots are encrypted
client-side with a key the product owner does not hold (FR-O4).

## Reporting a problem

Security issues should go to the maintainer directly rather than into a public
issue tracker.
