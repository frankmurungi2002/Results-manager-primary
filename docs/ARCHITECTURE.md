# Architecture

## The stack, and why

**Tauri 2 + Rust + React/TypeScript + SQLite.**

A Ugandan school PC is a modest Windows machine with 8 GB of RAM that also runs
the office's other work. Tauri uses the WebView2 already on the machine instead
of shipping a browser, which makes the installer about 10 MB rather than 150 MB
and leaves far more headroom than Electron would. The backend is Rust, so the
code handling untrusted spreadsheets, image blobs and a school's entire academic
history is memory-safe by construction.

React gives the interface the 30-minute-demo quality bar SRS 14.6 asks for
without hand-rolling a widget toolkit. SQLite is the right database for a
single-PC offline product: one file, no service to install or keep running, and
a genuine online-backup API.

Styling is hand-written CSS over a token layer rather than a utility framework —
the whole application resolves to the tokens in `src/styles/tokens.css`, which
is what keeps the light and dark themes genuinely designed rather than inverted.

## Two halves, one boundary

```
┌─────────────────────────────┐
│  WebView2 (React)           │   renders; holds no credential,
│  ─────────────────────────  │   no database handle, no file access
│  screens/ components/       │
│  lib/api.ts                 │   the only way out
└──────────┬──────────────────┘
           │  named Tauri commands
┌──────────▼──────────────────┐
│  Rust                       │
│  commands/   permission checks, validation
│  domain/     grading, ids, read models
│  security/   Argon2id, the session guard
│  audit.rs    written in the caller's transaction
│  db/         one connection, versioned migrations
└──────────┬──────────────────┘
           │
      school.rmdb  +  backups/
```

Every command resolves the actor from Rust memory. The interface never says who
it is; it asks.

## Decisions worth knowing

### One connection, one mutex

A school runs RM on one PC with at most a handful of concurrent operations, so a
connection pool buys nothing and costs clarity. A single `Mutex<Connection>`
makes transaction boundaries obvious and makes it impossible to write an action
and its audit entry through different connections.

### The audit trail is in the data layer, not bolted on

SRS 16.6 says the audit log should be close to free *if designed in up front*.
So `audit::record` takes the same connection the write took and is called inside
the caller's transaction. An action and its record commit together or not at
all. There is no "remember to log this" convention to forget.

### Nothing is ever deleted

Learners are dropped, staff and subjects are retired, enrollments are closed,
classes are retired. Every table that can hold history has a `status` column
instead of a `DELETE`. This is cheap when it is in the data model from the start
and impossible to retrofit honestly — which is exactly what SRS 16.6 says about
it.

### Percentages, not raw marks, drive grading

A mark is converted to a percentage of *its subject's* maximum before any band
is consulted. That single rule is what makes FR-C13's per-subject maximum
override safe: a subject marked out of 40 grades identically to one marked out
of 100, and no band table has to know about either.

### One term is open at a time

Opening a term closes whichever was open. "Where do I enter marks?" then has
exactly one answer, and a mark cannot be filed against last term by accident.
A closed term is read-only; reopening is a deliberate School Admin action that
is itself audited.

### A blank cell is not a zero

A mark that was never entered, an absence, and a score of zero are three
different things and stay three different things all the way to the report card.
The schema enforces it (`CHECK (is_absent = 1 OR score IS NOT NULL)`), the save
path deletes rather than zeroes on a blank, and the aggregate leaves absences
out instead of averaging them in.

### The print pipeline has no bypass

FR-D4 says no requirement may bypass the global print pipeline, so the pipeline
gives none. `DocumentEnvelope<T>` is the only shape a printable document takes,
its `branding` and `footer` are not optional and are filled by the backend, and
there is no field anywhere for a sponsor or third-party name. On the interface
side, exactly one component renders a sheet.

### Ranking follows the grading system

Where the grading system has points (UNEB), a lower aggregate ranks first.
Where it does not, a higher mean percentage does. Equal results share a position
and the next one skips — 1, 2, 2, 4 — which is how a Ugandan report card reads.
A learner with no marks gets no position rather than last place.

### The division is only computed when it means something

The published UNEB boundaries are defined for four core subjects. With any other
number, RM shows the aggregate and stays quiet about the division rather than
inventing a boundary. Guessing here would produce a wrong, confidently-printed
number on a document a parent keeps.

## Where the SRS was deliberately not followed

Section 16 of the SRS argues against parts of itself, and this build takes that
advice:

| SRS says | Built instead | Why (SRS reference) |
| --- | --- | --- |
| Transactional dual-SSD mirror with checksummed read-back (FR-B12) | Scheduled online-backup snapshot to a second drive | 16.1 — hand-building RAID-1 risks silently corrupt report cards, the worst failure for a product selling data safety |
| Nine worldwide grading presets (4.2) | The two Uganda uses, plus Percentage Only and the Custom Band Builder | 16.2 — guessed thresholds for markets nobody is buying yet |
| Owner admin panel with three dashboards (FR-O1–O3) | Not built | 16.3 — worth it at 50+ schools, not at 3 |
| Payment gateway and hardware fingerprint (FR-W4, FR-B0) | Manual activation | 16.4 — generates lockouts and support tickets before there is volume to justify it |
| Full request/approve/reverse late-marks workflow (FR-C14) | Reopen the term, audited | 16.5 — same safety property, far less UI |

Each of these is a Phase 2 or Phase 3 item in SRS Section 17, not a gap.

## Adding to it

**A new screen**: add a `ScreenId` in `state/store.ts`, an entry in
`NAV_SECTIONS` in `components/AppShell.tsx`, and a case in `renderScreen` in
`App.tsx`.

**A new command**: write it in the right `commands/` module, resolve the session
first (`state.sessions.require()?`), check permission, validate, write, audit,
then register it in `lib.rs` and add a typed wrapper in `src/lib/api.ts`.

**A schema change**: append a new `Migration` to `MIGRATIONS` in
`db/migrations.rs` with the next version number. Never edit an applied
migration — a school's database is already past it.

**A new document**: add a body type and a command that ends in `envelope(...)`,
then a sheet component in `PrintDocument.tsx`. Do not render printable markup
anywhere else.
