# Results Manager

Offline-first school management for Ugandan nursery and primary schools.

> Marks in. Reports out. Offline.

A school pays once a term, installs Results Manager on one Windows PC, and
produces every printed output a parent or a PLE administrator expects —
registration records, mark sheets, class lists, registers and report cards —
without ever needing a reliable internet connection.

Built against the consolidated SRS (Nursery + Primary edition), following the
phased plan in its Section 17. This repository is **Phase 0 — Pilot MVP**.

---

## Running it

You need [Node 20+](https://nodejs.org) and
[Rust](https://rustup.rs) (stable), plus the Windows build tools Tauri needs:
[Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
and [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)
(already present on Windows 11 and on updated Windows 10).

```bash
npm install          # once
npm run app          # development, with hot reload
npm run app:build    # produces a Windows installer in src-tauri/target/release/bundle
```

The first run opens the setup wizard. Everything it asks can be changed later.

### Where the data lives

`%APPDATA%\ug.resultsmanager.desktop\`

- `school.rmdb` — the live database
- `backups\` — the twenty most recent snapshots
- `exports\`

Nothing leaves this folder unless someone exports or backs up.

---

## What is built

| Requirement | What it does |
| --- | --- |
| FR-B1, FR-C1, FR-C2 | One login frame, role-based routing, scoped teacher access, dual Subject/Class Teacher mode |
| FR-B2 | Institution name (required), logo, accent colour, applied everywhere |
| FR-B3 | Academic calendar with Ugandan defaults — 3 terms, BOT/MID/EOT |
| FR-B4 | Up to three School Admins, enforced in the backend |
| FR-B5 | Class ladder, institutional subject catalogue, registration-number generator |
| FR-B6 | Teacher assignment; one Class Teacher and one Assistant per class, enforced by a database index |
| FR-B7 | Optional features panel (photos ready; permits, streams, weekly assignments stubbed) |
| FR-B8 | Fees block — flagged learners are excluded from printing and listed, never silently skipped |
| FR-B9 | Genuinely designed light and dark themes, plus "match Windows" |
| FR-B10 | Backup status; cloud controls visible and marked coming soon |
| FR-B12 | Simplified per SRS 16.1 — scheduled snapshot to a second drive, not a hand-built RAID |
| FR-C3 | Marks entry: in-app grid and guided form, one shared validator and save path |
| FR-C5 | Subject analytics — mean, pass rate, range, grade distribution |
| FR-C6 | Per-subject marks-entry progress |
| FR-C7, FR-C8 | Search by name or registration number; audited bio-data |
| FR-C10 | Add and drop — never delete; readmission keeps the history |
| FR-C13 | Per-class subject rename, grading override and maximum, on a stable subject id |
| FR-D1 | Partial and final report cards with aggregate, division, position and comments |
| FR-D2 | Class lists |
| FR-D4 | The global print pipeline — one component, institution branding only, no bypass |
| FR-E2 | Append-only audit log, enforced by database triggers |
| FR-G11 | Comment bank, pre-loaded, editable per learner |
| FR-G12 | Bulk print for a whole class with a skipped-learner summary |
| SRS 4.2 | Grading engine, UNEB Primary and Nursery Descriptive presets, Custom Band Builder |

### Not built yet

Following SRS 16 and 17 deliberately: no payment gateway, no hardware
fingerprint, no transactional dual-SSD mirror, no owner admin panel, no
worldwide grading presets beyond the two Uganda actually uses. Excel upload
(the third FR-C3 method), exam permits, streams, weekly assignments, promotion,
ID cards and onboarding import are the next items in the plan.

---

## Layout

```
src/                     React + TypeScript interface
  components/            Shared primitives, the app shell, the print pipeline
  screens/               One file per screen
  lib/                   Typed IPC and the shapes that cross it
  state/                 Session, navigation, theme, toasts
  styles/                Design tokens, then everything built from them

src-tauri/               Rust backend
  src/db/                Connection, pragmas, versioned migrations
  src/domain/            Grading engine, ids, read models, setup defaults
  src/security/          Argon2id hashing, the session guard
  src/commands/          One module per area; every command permission-checked
  src/audit.rs           The audit trail, written inside the caller's transaction
```

## Testing

```bash
python3 scripts/verify.py      # schema, every SQL query, and the data guarantees
cd src-tauri && cargo test     # grading, ranking, passwords, sessions, migrations
npm run typecheck              # the whole interface
```

`scripts/verify.py` needs nothing but Python 3 — no Rust toolchain, no
`npm install`. It builds the schema from the migrations, compiles every SQL
statement in the Rust against it, and then attacks the result to confirm the
guarantees hold. Run it after touching `migrations.rs` or any query; a wrong
column name compiles fine in Rust and fails in front of a headteacher.

The grading engine, class ranking, password rules, session scoping and the
schema's own guarantees (an append-only audit log, complete band coverage) are
covered by tests, because those are the parts where a silent bug shows up a term
later as a wrong report card.

---

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — how it fits together and why
- [`docs/SECURITY.md`](docs/SECURITY.md) — the security model, and how to turn on database encryption
- [`docs/SRS.md`](docs/SRS.md) — the consolidated specification this is built from
