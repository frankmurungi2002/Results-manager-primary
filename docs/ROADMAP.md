# Roadmap

The complete requirement register for Results Manager, with every requirement's
current state in this repository and the milestone it belongs to.

- The **source of truth for what a requirement means** is [`SRS.md`](SRS.md).
  This document does not restate the spec; it tracks it.
- The **build order** follows SRS Section 17, with its Section 16 cuts applied,
  plus a stabilisation milestone (M0) that the SRS could not anticipate.
- Statuses were taken from the code, not from the README, on 2026-09-27.
  Update the row in the same commit that changes the status.

## Status legend

| Status | Meaning |
| --- | --- |
| ✅ Built | Backend, interface and tests exist; usable end to end |
| 🟡 Partial | Usable, but part of the requirement is missing (the gap is named) |
| 🔌 Backend only | Commands exist and are wrapped in `api.ts`, but no screen uses them |
| 🧱 Schema only | Tables or flags exist; no commands, no interface |
| ⬜ Not started | Nothing yet |
| ⏸ Deferred | Deliberately postponed by SRS 16; the simplified form is noted |
| 🌐 Other system | Belongs to the website or owner panel, not this desktop repository |

## Milestones at a glance

| Milestone | Goal | Exit criteria |
| --- | --- | --- |
| **M0 — Stabilise** | What is already here actually works | `scripts/verify.py` passes, `cargo test` passes, `npm run typecheck` passes, a clean install signs in and prints a report card, and all three run in CI |
| **M1 — Finish the pilot (SRS Phase 0)** | MB Primary School runs one full term on RM | Every Phase 0 row below is ✅ and one real term's report cards were printed from RM |
| **M2 — First paying cohort (SRS Phase 1)** | About 20 schools, manual activation | Every Phase 1 row is ✅ and subscription enforcement (FR-B11) is live |
| **M3 — Scale-up (SRS Phase 2)** | About 100 schools, automated payments | Every Phase 2 row is ✅ |
| **M4 — Mature product (SRS Phase 3)** | Long-term features that need data or scale | Built as demand appears |
| **Later** | SRS 12.1, 15 and 15.2 items | Only when a real school asks |

---

## M0 — Stabilise what exists

These are defects and gaps in code that already exists. They come first
because each one breaks something the README already lists as built.

| # | Item | Where | Why it matters |
| --- | --- | --- | --- |
| M0.1 | `SessionView` is sent with snake_case fields, but the interface reads camelCase (`isAdmin`, `fullName`, `classIds`, ...) | `src-tauri/src/security/session.rs:117`; fix with `#[serde(rename_all = "camelCase")]` | After sign-in every role flag is `undefined`, so routing and permissions in the interface break. Caught by `verify.py` |
| M0.2 | The PLE projection query filters on `exams.kind`, a column that does not exist | `src-tauri/src/commands/insights.rs:132` | The command fails at runtime. Caught by `verify.py` |
| M0.3 | Backups only run when someone clicks the button; nothing schedules them | `commands/system.rs` `run_backup`, `db/mod.rs` `backup_to` | Simplified FR-B12 (SRS 16.1) promises a *scheduled* copy, every 15 minutes and on close |
| M0.4 | There is no way to restore a backup | none | A backup that cannot be restored by a headteacher is not a backup. Needed for FR-E4 scenario 1 |
| M0.5 | CI: run `verify.py`, `cargo test`, `npm run typecheck` and `cargo clippy` on every push | `.github/workflows/` (new) | M0.1 and M0.2 reached `main` because nothing runs these automatically |
| M0.6 | README says onboarding import is not built, but its backend exists | `README.md` | Keep the "what is built" table honest |
| M0.7 | A first real Windows build and install of the app, checked by hand | `npm run app:build` | Nothing has been proven outside a dev machine yet |

---

## Full requirement register

Every requirement in the SRS, grouped by its SRS phase. "Milestone" is where it
lands in this plan.

### Phase A — RM Official Website (SRS 5)

The website is a separate system. None of it lives in this repository, and for
the pilot SRS 16.4 replaces it with manual payment and a manual activation key.

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-W1 | Public landing and product information | 🌐 ⬜ | M2 | A static page is enough for the first cohort |
| FR-W2 | Two plans: Termly (~120 days) and Annually (365) | 🌐 ⬜ | M2 (as a price list), M3 (in checkout) | |
| FR-W3 | Sign-up with level and full geography | 🌐 ⬜ | M3 | Capture geography by hand from M2 so FR-O2 has data later |
| FR-W4 | Ugandan payment gateway (Mobile Money, cards) | 🌐 ⏸ | M3 | Manual Mobile Money to a business number until then (SRS 16.4) |
| FR-W5 | Automatic first-admin credentials after payment | 🌐 ⏸ | M3 | Pilot: the setup wizard creates the first admin |
| FR-W6 | Level-specific installer with a one-time install token | 🌐 ⏸ | M3 | Pilot: activation key tied to institution ID |
| FR-W7 | Connector-RM rescue tool | 🌐 ⏸ | M4 | |
| FR-W8 | Encrypted cloud backup endpoint, built but switched off | 🌐 ⏸ | M4 | |
| FR-W9 | Institution online account | 🌐 ⬜ | M3 | |
| FR-W10 | Admin manual, optional quiz and certificate | 🌐 ⬜ | Manual: M1. Quiz and certificate: M4 | The manual is what makes SRS 14.4 (setup without a technician) achievable |
| FR-W11 | Public feedback form | 🌐 ⬜ | M3 | |

### Phase B — Institutional configuration (SRS 6)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-B0 | Hardware binding (PC lock) | ⏸ | M4 | SRS 16.4: licence key tied to institution ID instead |
| FR-B1 | One login frame, role-based routing | ✅ (blocked by M0.1) | M0 | |
| FR-B2 | Institution name, logo, accent colour | ✅ | — | |
| FR-B3 | Academic calendar: 3 terms, BOT/MID/EOT | ✅ | — | Opening a term closes the others |
| FR-B4 | Up to three School Admins | ✅ | — | Enforced in the backend |
| FR-B5 | Class ladder, subject catalogue, registration numbers | ✅ | — | |
| FR-B6 | Teacher assignment; one Class Teacher and one Assistant per class | ✅ | — | Enforced by a unique index |
| FR-B7 | Optional features: Photos, Exam Permits, Streams, Weekly Assignments | 🟡 | M2–M3 | Flags are stored; Photos works; the other three have nothing behind them yet |
| FR-B8 | Fees-block rule at print time | ✅ (manual flag) | M2 reads from the Fees Ledger | |
| FR-B9 | Light, dark and match-Windows themes | ✅ | — | |
| FR-B10 | Backup status; cloud controls marked "coming soon" | ✅ | — | |
| FR-B11 | Subscription tracker, 7-day grace, then read-only mode | ⬜ | **M2** | SRS 16.6: build as specified from day one of paid use. Needs an offline-verifiable signed expiry in the activation key |
| FR-B12 | Backup to a second drive | 🟡 simplified | M0 (schedule + restore), M3 (full mirror) | See M0.3 and M0.4. The full checksummed dual-SSD mirror (SRS 13) is M3 |
| FR-B13 | Software update channel with rollback | ⏸ | M3 | Manual, support-assisted reinstall until then. Tauri's updater plugin is the natural route |
| FR-B14 | In-app feedback | ⬜ | M2 (email/WhatsApp link), M3 (queued form + screenshot) | |

### Phase C — Daily operations (SRS 7)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-C1 | Teachers see only their assigned classes and subjects | ✅ | — | |
| FR-C2 | Subject Teacher / Class Teacher mode | ✅ | — | |
| FR-C3 | Marks entry: Excel upload, in-app grid, guided form | 🟡 | **M1** | Grid and form done. **Excel upload missing**: export a pre-filled template per class/subject/exam, read it back through the same validator and save path. SRS 14.7 requires all three to be equally reliable |
| FR-C5 | Subject analytics | ✅ | — | |
| FR-C6 | Marks-entry deadline and per-teacher progress | ✅ | — | |
| FR-C7 | Search by name or registration number | ✅ | — | |
| FR-C8 | Audited bio-data editing | ✅ | — | |
| FR-C9 | Correction messages from Subject Teacher to Class Teacher | ⬜ | M2 | Needs a small inbox: message, learner, status (open/resolved) |
| FR-C10 | Add and drop learners, never delete | ✅ | — | Includes readmit and transfer |
| FR-C11 | Streams (up to 20 per class) | 🧱 | **M2** | `streams` table and stream-scoped assignment index exist. Needs commands, roster/marks/report scoping and screens. SRS 16.5 says most schools need this |
| FR-C12 | Weekly assignments | ⬜ (flag only) | M3 | |
| FR-C13 | Per-class subject rename, grading override, maximum | ✅ | — | |
| FR-C14 | Late marks for a closed term | 🟡 simplified | M2 (simplified), M3 (full workflow) | Today: an admin reopens the whole term. M2: an admin edits one closed mark with a mandatory, audited reason (SRS 16.5). M3: request → approve → 24-hour reversal |

### Phase D — Outputs (SRS 8)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-D1 | Partial and final report cards | ✅ | M3 adds the weekly-assignment back page | Aggregate, division, position, comments |
| FR-D2 | Exam permits, class lists, mark lists, mark sheets, registers | 🟡 | **M2** | Class lists done. Missing: exam permits (gated by the toggle, dated from FR-G7 in M3), mark lists, mark sheets, exam registers, attendance registers |
| FR-D4 | Global print pipeline, institution branding only | ✅ | — | Every new document must go through `DocumentEnvelope` |

### Phase E — Lifecycle, audit, recovery, history (SRS 9)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-E1 | Rank classes by learners gained and lost | ⬜ | M2 | The data exists in `enrollments`; this is a query and a dashboard card |
| FR-E2 | Append-only audit log | ✅ | — | Enforced by triggers |
| FR-E3 | Promotion up the ladder; sealed yearly records; graduates | ⬜ | **M2** | Schema allows `promoted` and `graduated`. Needs: new academic year creation, bulk promote with per-learner exceptions (repeat, leave), sealing the old year, PLE candidate list |
| FR-E4 | Disaster recovery, three scenarios | 🟡 | M0 (scenario 1), M4 (2 and 3) | Scenario 1 needs restore (M0.4). Scenarios 2 and 3 depend on FR-W7 and FR-W8 |
| FR-E5 | Multi-year subject trends | ⬜ | M4 | Needs two or more sealed years to be meaningful |
| FR-E6 | Multi-year weekly-assignment trends | ⬜ | M4 | Needs FR-C12 |

### Phase F — Owner admin panel (SRS 10)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-O0 | Single owner account, password + TOTP | 🌐 ⬜ | M3 | A principle from day one, even if the panel is a spreadsheet |
| FR-O1 | Revenue dashboard | 🌐 ⏸ | M4 | A spreadsheet until ~50 schools (SRS 16.3) |
| FR-O2 | Geographic dashboard | 🌐 ⏸ | M4 | |
| FR-O3 | Growth and churn dashboard | 🌐 ⏸ | M4 | |
| FR-O4 | Privacy boundary: no institutional data leaves the school | Principle | Always | Already true: the app makes no network calls |

### Phase G — Extended school operations (SRS 11)

| ID | Requirement | Status | Milestone | Notes |
| --- | --- | --- | --- | --- |
| FR-G1 | Document generator: Custom Builder / Template Upload, letters, certificates | ⬜ | **M1** (Custom Builder), M3 (Template Upload + letters + certificates) | Must reuse `DocumentEnvelope`; the report card becomes one template of this engine |
| FR-G2 | Visitor log and visitation slip | ⬜ | M2 | |
| FR-G3 | School overview dashboard | 🟡 | M4 | `dashboard_summary` exists; the full version waits for attendance, fees and staff data |
| FR-G4 | Staff HR: attendance, leave, contracts (no payroll) | ⬜ | M3 | |
| FR-G5 | Fees ledger; drives the fees block automatically | ⬜ | **M2** | Fee per class per term, payments, running balance; FR-B8 then reads the balance |
| FR-G6 | Timetable with double-booking check | ⬜ | M3 | |
| FR-G7 | Shared exams and academic calendar | 🟡 | M3 | A calendar screen and `exams.scheduled_date` exist; holidays, events and per-subject exam times do not |
| FR-G8 | SMS to parents (fees, results, absence) | ⬜ | M3 | Needs a local send queue; gateway choice is an open decision |
| FR-G9 | Daily attendance register | 🧱 | **M1** | `attendance` table exists; needs commands, a fast class register screen, the "days present" figure on the report card |
| FR-G10 | Full data export to Excel/CSV | ⬜ | M2 | `rust_xlsxwriter` is already a dependency |
| FR-G11 | Comment bank | ✅ | — | |
| FR-G12 | Bulk print with skipped-learner summary | ✅ (class scope) | M2 adds stream and whole-school scope, with progress | |
| FR-G13 | Onboarding import of learners and staff | 🔌 | **M1** | Learner import backend is built (two-pass, validated). Missing: the screen, and staff import |
| FR-G14 | Discipline and conduct log | ⬜ | M3 | |
| FR-G15 | Boarding module (toggle) | ⬜ | M4 | Only when a boarding school signs up |
| FR-G16 | Student and staff ID cards | ⬜ | **M1** | A founding requirement; single and batch, PVC size and A4 sheet |
| FR-G17 | Admissions pipeline | ⬜ | M2 | |
| FR-G18 | Correspondence log | ⬜ | M4 | |
| FR-G19 | Meeting minutes and action points | ⬜ | M4 | |
| FR-G20 | Petty cash and expenditure | ⬜ | M3 | |
| FR-G21 | School store and uniform tracking | ⬜ | M3 | |
| FR-G22 | Pass-out / exit pass with guardian SMS | ⬜ | M3 | Needs FR-G8 and FR-G2 |
| FR-G23 | Bulk SMS broadcast with cost estimate | ⬜ | M3 | Built with FR-G8 |

### Not in the SRS numbering, but already in the code

| Item | Status | Milestone | Notes |
| --- | --- | --- | --- |
| PLE projection | 🔌, broken query (M0.2) | M1 | Useful sales-demo material for P7 classes |
| Subject heatmap | 🔌 | M1 | |

### Non-functional requirements (SRS 14)

These are standing requirements, not milestones. Each needs a way to check it.

| Ref | Requirement | Status | How we will check it |
| --- | --- | --- | --- |
| 14.1 | All daily work runs with no internet | ✅ by design | The app makes no network calls; keep it that way until FR-B11/FR-B13/FR-G8 add deliberate, optional ones |
| 14.2 | No single hardware failure destroys history | 🟡 | M0.3 + M0.4 make this true for the pilot; the full mirror in M3 |
| 14.3 | Scoped access, hashed credentials, encryption | ✅ | Argon2id, session guard, optional SQLCipher (`docs/SECURITY.md`) |
| 14.4 | A headteacher sets it up in a few hours without a technician | ⬜ untested | Manual (FR-W10) plus a timed first-run test with a non-technical person in M1 |
| 14.5 | Every write audited; nothing hard-deleted | ✅ | `verify.py` guarantees; add a check that every new command writes an audit row |
| 14.6 | Premium UI, 1366×768 to large monitors, no freezing with 800 learners | 🟡 untested | M1: a seeded 800-learner database, a timing check on roster, marks sheet, search and bulk print, and screenshots at 1366×768 |
| 14.7 | Every module and every input method to the same quality | Standing | Review rule: a new input path shares the validator and save path of the existing one |

### Named but not specified (SRS 12.1, 15, 15.2, 15.3)

Recorded so they are not forgotten; none is scheduled.

| Item | Source | Condition to start |
| --- | --- | --- |
| Library (book issue and return) | 12.1 | A school asks |
| Sports / co-curricular records | 12.1 | A school asks |
| Sick-bay health log | 12.1 | A school asks; health data needs its own privacy review |
| Transport (routes, fuel, maintenance) | 12.1 | A school with its own buses asks |
| Anonymised / blind marking | 15.2 | Must be designed into marks entry, not bolted on |
| Parent access via SMS code | 15.2 | After FR-G8 |
| Payroll-provider export | 15.1 | After FR-G4; RM never computes PAYE/NSSF itself |
| Secondary Level Pack | 15.3 | Primary stable through full years **and** funding or demand. Needs per-learner subject sets, which is a data-model redesign, not a preset |
| University Level Pack | 15 | Same conditions |
| Teacher mobile app | 15 | Dropped; revisit only on repeated real demand |

---

## Requirements the SRS does not state but the product needs

Found while reading the code against the spec. Each should be added to the SRS
or consciously rejected.

| # | Need | Why | Suggested milestone |
| --- | --- | --- | --- |
| X1 | **Restore from backup** in the interface, with a preview of what the backup contains and its date | The SRS specifies backing up but never restoring | M0 |
| X2 | **Admin locked out**: recovery when the only School Admin forgets their password | A second admin can reset (FR-B4), but a school with one admin has no path. Options: a recovery code printed at setup, or support-issued reset tied to the activation key | M1 |
| X3 | **Start a new academic year** (years, terms, exams created from the previous year's pattern) | FR-B3 covers the first year; nothing covers the second | M2, with FR-E3 |
| X4 | **Save as PDF** for every document, not only paper | Schools email and WhatsApp report cards | M1 |
| X5 | **Printer and paper handling**: A4 vs A5 report cards, margins, one learner per page | Report cards are the product; print layout problems appear on real printers only | M1 |
| X6 | **Seeded demo data** for sales demos and performance tests | SRS 14.6 says the 30-minute demo decides the sale | M1 |
| X7 | **Error reporting**: a local log file the school can send to support | Offline product; support needs something to read | M1 |
| X8 | **Installer signing** on Windows | Unsigned installers trigger SmartScreen warnings, which frighten a headteacher | M2 |
| X9 | **Data retention for dropped learners and retired staff** | Never-delete keeps data forever; a school may need to answer what is kept and why | M3 |
| X10 | **Class-level position and subject position on report cards** (a common Ugandan request) | Confirm with the pilot school | M1 decision |

---

## Decisions needed from the product owner

These block or shape specific rows above.

1. **Pilot scope**: confirm SRS Phase 0 as the M1 scope, or trim it. In particular, the Custom Report Builder (FR-G1) is the largest M1 item; a fixed, well-designed report card could ship first.
2. **Activation and subscription (FR-B11)**: format of the offline activation key, and who issues it during M2.
3. **SMS gateway (FR-G8)**: Africa's Talking or EgoSMS, and who pays for messages.
4. **Report card layout**: does the pilot school want position per subject, A4 or A5, and a photo on the card?
5. **Streams timing**: SRS 16.5 moves streams to Phase 1 (M2). If MB Primary runs streams, it has to move into M1.
6. **X2 admin recovery**: recovery code or support-issued reset.
7. **Excel marks template (FR-C3)**: one sheet per subject, or one sheet per class with all subjects.

---

## Suggested order inside M1

Dependencies first, then the highest daily-use value:

1. M0 items (everything else depends on sign-in and backups working)
2. FR-G13 onboarding import screen (the pilot's data goes in first)
3. FR-G9 daily attendance (cheap, daily use, feeds the report card)
4. FR-C3 Excel marks upload
5. X4/X5 PDF and print layout on real printers
6. FR-G16 ID cards
7. FR-G1 Custom Report Builder
8. PLE projection and subject heatmap screens
9. X6 demo data and the SRS 14.6 performance check at 800 learners
