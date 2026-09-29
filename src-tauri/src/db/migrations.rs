//! Forward-only, versioned schema migrations.
//!
//! Rules this file exists to enforce:
//!
//! * **Nothing is ever hard-deleted.** Learners, teachers, subjects and classes
//!   carry a `status` column and are retired, dropped or superseded — never
//!   removed. (SRS 14.5)
//! * **Every write is auditable.** `audit_log` is append-only, enforced by
//!   triggers rather than by convention. (FR-E2)
//! * **Subject identity is stable.** A subject's id survives renames so
//!   multi-year history does not break when a class renames it. (FR-C13)

use rusqlite::Connection;

use crate::error::AppResult;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "core_schema",
        sql: M001_CORE_SCHEMA,
    },
    Migration {
        version: 2,
        name: "seed_grading_and_comments",
        sql: M002_SEED,
    },
    Migration {
        version: 3,
        name: "nursery_report",
        sql: M003_NURSERY_REPORT,
    },
    Migration {
        version: 4,
        name: "staff_photos",
        sql: M004_STAFF_PHOTOS,
    },
    Migration {
        version: 5,
        name: "pass_outs_and_sms",
        sql: M005_PASS_OUTS_AND_SMS,
    },
    Migration {
        version: 6,
        name: "weekly_assignments",
        sql: M006_WEEKLY_ASSIGNMENTS,
    },
    Migration {
        version: 7,
        name: "timetables",
        sql: M007_TIMETABLES,
    },
];

/// Applies every migration this binary knows about that the database has not
/// seen yet, each in its own transaction.
pub fn run(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL
         );",
    )?;

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for migration in MIGRATIONS {
        if migration.version <= current {
            continue;
        }

        log::info!(
            "applying migration {} ({})",
            migration.version,
            migration.name
        );

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                migration.version,
                migration.name,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        tx.commit()?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 001 — core schema
// ---------------------------------------------------------------------------

const M001_CORE_SCHEMA: &str = r#"

-- ===========================================================================
-- Institution identity (FR-B2). Exactly one row, ever.
-- ===========================================================================
CREATE TABLE institution (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    name                TEXT    NOT NULL,
    motto               TEXT,
    logo_png            BLOB,
    accent_color        TEXT    NOT NULL DEFAULT '#4F63D2',
    address             TEXT,
    phone               TEXT,
    email               TEXT,
    -- FR-B5: registration-number format, unique institution-wide.
    reg_number_pattern  TEXT    NOT NULL DEFAULT '{YEAR}/{SEQ:4}',
    reg_number_next     INTEGER NOT NULL DEFAULT 1,
    setup_completed_at  TEXT,
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL
);

-- Free-form key/value store for feature toggles (FR-B7) and preferences.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ===========================================================================
-- People who sign in (FR-B1, FR-B4, FR-B6)
-- ===========================================================================
CREATE TABLE users (
    id                   TEXT PRIMARY KEY,
    username             TEXT NOT NULL COLLATE NOCASE,
    full_name            TEXT NOT NULL,
    email                TEXT,
    phone                TEXT,
    role                 TEXT NOT NULL CHECK (role IN ('school_admin', 'teacher')),
    password_hash        TEXT NOT NULL,
    must_change_password INTEGER NOT NULL DEFAULT 1,
    -- Retired, never deleted (SRS 14.5).
    status               TEXT NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active', 'retired')),
    failed_attempts      INTEGER NOT NULL DEFAULT 0,
    locked_until         TEXT,
    last_login_at        TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_users_username ON users (username COLLATE NOCASE);
CREATE INDEX idx_users_role ON users (role, status);

-- FR-B4: at most three School Admins. Enforced in Rust on insert; this view
-- makes the count cheap to read.
CREATE VIEW v_active_school_admins AS
    SELECT * FROM users WHERE role = 'school_admin' AND status = 'active';

-- ===========================================================================
-- Grading systems library (SRS 4.2)
-- ===========================================================================
CREATE TABLE grading_systems (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    kind        TEXT NOT NULL CHECK (kind IN ('numeric', 'descriptive', 'letter', 'percentage')),
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    -- A preset ships read-only; the Custom Band Builder clones it to edit.
    is_editable INTEGER NOT NULL DEFAULT 1,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE grading_bands (
    id                TEXT PRIMARY KEY,
    grading_system_id TEXT NOT NULL REFERENCES grading_systems (id),
    label             TEXT NOT NULL,
    -- Percentage of the subject maximum. Inclusive lower, inclusive upper.
    lower_bound       REAL NOT NULL,
    upper_bound       REAL NOT NULL,
    -- Grade points, where the system has them (UNEB aggregate, GPA, UCE).
    points            REAL,
    remark            TEXT,
    position          INTEGER NOT NULL,
    CHECK (lower_bound <= upper_bound)
);

CREATE INDEX idx_grading_bands_system ON grading_bands (grading_system_id, position);

-- ===========================================================================
-- Academic calendar (FR-B3)
-- ===========================================================================
CREATE TABLE academic_years (
    id         TEXT PRIMARY KEY,
    label      TEXT NOT NULL UNIQUE,
    start_date TEXT,
    end_date   TEXT,
    status     TEXT NOT NULL DEFAULT 'planning'
                    CHECK (status IN ('planning', 'active', 'sealed')),
    sealed_at  TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE terms (
    id               TEXT PRIMARY KEY,
    academic_year_id TEXT NOT NULL REFERENCES academic_years (id),
    seq              INTEGER NOT NULL,
    name             TEXT NOT NULL,
    start_date       TEXT,
    end_date         TEXT,
    status           TEXT NOT NULL DEFAULT 'planning'
                          CHECK (status IN ('planning', 'open', 'closed')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    UNIQUE (academic_year_id, seq)
);

CREATE TABLE exams (
    id             TEXT PRIMARY KEY,
    term_id        TEXT NOT NULL REFERENCES terms (id),
    seq            INTEGER NOT NULL,
    code           TEXT NOT NULL,          -- BOT / MID / EOT, or a school's own
    name           TEXT NOT NULL,
    -- Share of the term mark, 0..1. Zero means "not counted in the aggregate".
    weight         REAL NOT NULL DEFAULT 0,
    -- The set that produces the end-of-term report card (FR-D1).
    is_final       INTEGER NOT NULL DEFAULT 0,
    scheduled_date TEXT,                   -- feeds exam permits (FR-G7)
    status         TEXT NOT NULL DEFAULT 'planning'
                        CHECK (status IN ('planning', 'open', 'closed')),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE (term_id, code)
);

CREATE INDEX idx_terms_year ON terms (academic_year_id, seq);
CREATE INDEX idx_exams_term ON exams (term_id, seq);

-- ===========================================================================
-- Classes, streams, subjects (FR-B5, FR-C11, FR-C13)
-- ===========================================================================
CREATE TABLE classes (
    id                        TEXT PRIMARY KEY,
    code                      TEXT NOT NULL UNIQUE,
    name                      TEXT NOT NULL,
    level_kind                TEXT NOT NULL CHECK (level_kind IN ('nursery', 'primary')),
    -- Position on the promotion ladder: Baby=1 .. P7=10. Survives renaming.
    ladder_position           INTEGER NOT NULL,
    default_grading_system_id TEXT REFERENCES grading_systems (id),
    status                    TEXT NOT NULL DEFAULT 'active'
                                   CHECK (status IN ('active', 'retired')),
    created_at                TEXT NOT NULL,
    updated_at                TEXT NOT NULL
);

CREATE INDEX idx_classes_ladder ON classes (ladder_position);

CREATE TABLE streams (
    id         TEXT PRIMARY KEY,
    class_id   TEXT NOT NULL REFERENCES classes (id),
    name       TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (class_id, name)
);

-- The institutional subject catalogue. `id` is the stable identity that
-- per-class renames must not break (FR-C13).
CREATE TABLE subjects (
    id         TEXT PRIMARY KEY,
    code       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    max_score  REAL NOT NULL DEFAULT 100,
    pass_mark  REAL NOT NULL DEFAULT 50,
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- A subject as it is actually taught in one class: optional display rename,
-- optional grading override, its own maximum (FR-C13).
CREATE TABLE class_subjects (
    id                TEXT PRIMARY KEY,
    class_id          TEXT NOT NULL REFERENCES classes (id),
    subject_id        TEXT NOT NULL REFERENCES subjects (id),
    display_name      TEXT,
    grading_system_id TEXT REFERENCES grading_systems (id),
    max_score         REAL,
    pass_mark         REAL,
    position          INTEGER NOT NULL DEFAULT 0,
    -- Counts toward the UNEB division aggregate. In Ugandan primary that is
    -- the four core subjects; everything else is reported but not aggregated.
    is_core           INTEGER NOT NULL DEFAULT 1,
    -- Removing a subject hides it going forward; historical marks are untouched.
    status            TEXT NOT NULL DEFAULT 'active'
                           CHECK (status IN ('active', 'retired')),
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    UNIQUE (class_id, subject_id)
);

CREATE INDEX idx_class_subjects_class ON class_subjects (class_id, position);

-- ===========================================================================
-- Teacher assignment (FR-B6, FR-C1)
-- ===========================================================================
CREATE TABLE teacher_assignments (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id),
    class_id   TEXT NOT NULL REFERENCES classes (id),
    stream_id  TEXT REFERENCES streams (id),
    -- NULL subject means the assignment is class-wide (class teacher).
    subject_id TEXT REFERENCES subjects (id),
    role       TEXT NOT NULL CHECK (role IN ('class_teacher', 'assistant_class_teacher', 'subject_teacher')),
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- FR-B6: exactly one Class Teacher and at most one Assistant per class, and
-- per stream when streams are on. Enforced by the database, not by hope.
CREATE UNIQUE INDEX idx_one_class_teacher_role
    ON teacher_assignments (class_id, IFNULL(stream_id, '-'), role)
    WHERE role IN ('class_teacher', 'assistant_class_teacher') AND status = 'active';

CREATE UNIQUE INDEX idx_one_subject_teacher
    ON teacher_assignments (class_id, IFNULL(stream_id, '-'), subject_id)
    WHERE role = 'subject_teacher' AND status = 'active';

CREATE INDEX idx_assignments_user ON teacher_assignments (user_id, status);
CREATE INDEX idx_assignments_class ON teacher_assignments (class_id, status);

-- ===========================================================================
-- Learners (FR-C8, FR-C10)
-- ===========================================================================
CREATE TABLE students (
    id                    TEXT PRIMARY KEY,
    reg_number            TEXT NOT NULL UNIQUE COLLATE NOCASE,
    full_name             TEXT NOT NULL,
    gender                TEXT CHECK (gender IN ('M', 'F')),
    date_of_birth         TEXT,
    lin                   TEXT,            -- UNEB Learner Identification Number
    photo_png             BLOB,
    guardian_name         TEXT,
    guardian_phone        TEXT,
    guardian_relationship TEXT,
    address               TEXT,
    -- FR-B8: a learner with incomplete fees gets no permit and no report card
    -- while the master switch is on. Replaced by a real balance in FR-G5.
    fees_blocked          INTEGER NOT NULL DEFAULT 0,
    fees_note             TEXT,
    -- Dropped, never deleted (FR-C10).
    status                TEXT NOT NULL DEFAULT 'active'
                               CHECK (status IN ('active', 'dropped', 'graduated')),
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);

CREATE INDEX idx_students_name ON students (full_name COLLATE NOCASE);
CREATE INDEX idx_students_status ON students (status);

-- One row per learner per academic year. The add/drop history lives here, so
-- FR-E1's gain/loss ranking is a query rather than a separate "folder".
CREATE TABLE enrollments (
    id               TEXT PRIMARY KEY,
    student_id       TEXT NOT NULL REFERENCES students (id),
    class_id         TEXT NOT NULL REFERENCES classes (id),
    stream_id        TEXT REFERENCES streams (id),
    academic_year_id TEXT NOT NULL REFERENCES academic_years (id),
    status           TEXT NOT NULL DEFAULT 'active'
                          CHECK (status IN ('active', 'dropped', 'promoted', 'graduated')),
    joined_at        TEXT NOT NULL,
    left_at          TEXT,
    drop_reason      TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    UNIQUE (student_id, academic_year_id)
);

CREATE INDEX idx_enrollments_class ON enrollments (class_id, academic_year_id, status);
CREATE INDEX idx_enrollments_student ON enrollments (student_id);

-- ===========================================================================
-- Marks (FR-C3)
-- ===========================================================================
CREATE TABLE marks (
    id               TEXT PRIMARY KEY,
    student_id       TEXT NOT NULL REFERENCES students (id),
    class_subject_id TEXT NOT NULL REFERENCES class_subjects (id),
    exam_id          TEXT NOT NULL REFERENCES exams (id),
    score            REAL,
    is_absent        INTEGER NOT NULL DEFAULT 0,
    entered_by       TEXT REFERENCES users (id),
    entered_at       TEXT NOT NULL,
    updated_by       TEXT REFERENCES users (id),
    updated_at       TEXT NOT NULL,
    CHECK (is_absent = 1 OR score IS NOT NULL),
    UNIQUE (student_id, class_subject_id, exam_id)
);

CREATE INDEX idx_marks_lookup ON marks (exam_id, class_subject_id);
CREATE INDEX idx_marks_student ON marks (student_id, exam_id);

-- ===========================================================================
-- Daily attendance register (FR-G9) — physical presence, distinct from the
-- weekly-assignment completion count in FR-C12.
-- ===========================================================================
CREATE TABLE attendance (
    id         TEXT PRIMARY KEY,
    student_id TEXT NOT NULL REFERENCES students (id),
    class_id   TEXT NOT NULL REFERENCES classes (id),
    stream_id  TEXT REFERENCES streams (id),
    term_id    TEXT NOT NULL REFERENCES terms (id),
    on_date    TEXT NOT NULL,              -- YYYY-MM-DD
    state      TEXT NOT NULL CHECK (state IN ('present', 'absent', 'late', 'excused')),
    note       TEXT,
    marked_by  TEXT REFERENCES users (id),
    marked_at  TEXT NOT NULL,
    UNIQUE (student_id, on_date)
);

CREATE INDEX idx_attendance_class_date ON attendance (class_id, on_date);
CREATE INDEX idx_attendance_term ON attendance (term_id, student_id);

-- ===========================================================================
-- Report card comments and the comment bank (FR-D1, FR-G11)
-- ===========================================================================
CREATE TABLE comment_bank (
    id         TEXT PRIMARY KEY,
    scope      TEXT NOT NULL CHECK (scope IN ('class_teacher', 'head_teacher')),
    category   TEXT,
    text       TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users (id),
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_comment_bank_scope ON comment_bank (scope, status);

CREATE TABLE report_comments (
    id                    TEXT PRIMARY KEY,
    student_id            TEXT NOT NULL REFERENCES students (id),
    term_id               TEXT NOT NULL REFERENCES terms (id),
    class_teacher_comment TEXT,
    head_teacher_comment  TEXT,
    updated_by            TEXT REFERENCES users (id),
    updated_at            TEXT NOT NULL,
    UNIQUE (student_id, term_id)
);

-- ===========================================================================
-- Audit log (FR-E2). Append-only, enforced by triggers.
-- ===========================================================================
CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    at         TEXT NOT NULL,
    actor_id   TEXT,
    actor_name TEXT,
    actor_role TEXT,
    action     TEXT NOT NULL,     -- e.g. 'marks.save', 'student.drop'
    entity     TEXT,              -- e.g. 'student'
    entity_id  TEXT,
    summary    TEXT NOT NULL,     -- one human-readable line
    details    TEXT               -- optional JSON
);

CREATE INDEX idx_audit_at ON audit_log (at DESC);
CREATE INDEX idx_audit_actor ON audit_log (actor_id, at DESC);
CREATE INDEX idx_audit_entity ON audit_log (entity, entity_id, at DESC);

CREATE TRIGGER trg_audit_no_update
BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER trg_audit_no_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;
"#;

// ---------------------------------------------------------------------------
// 002 — built-in grading systems and starter comment bank
// ---------------------------------------------------------------------------
//
// Per SRS 16.2, only the two systems a Ugandan primary school actually uses on
// day one ship with real thresholds, plus Percentage Only and the Custom Band
// Builder. The rest of the worldwide library (IB, Cambridge, GCSE, GPA) is
// deliberately not seeded with guessed boundaries.

const M002_SEED: &str = r#"

INSERT INTO grading_systems (id, code, name, description, kind, is_builtin, is_editable, status, created_at, updated_at) VALUES
 ('gs_uneb_primary', 'UNEB_PRIMARY', 'UNEB Primary',
  'Standard Ugandan primary grading, Division 1 (Distinction) through U. Default for P1-P7.',
  'numeric', 1, 0, 'active', datetime('now'), datetime('now')),
 ('gs_nursery_desc', 'NURSERY_DESC', 'Nursery Descriptive',
  'Five descriptive, non-numeric bands suited to early-childhood education. Default for Baby, Middle and Top.',
  'descriptive', 1, 0, 'active', datetime('now'), datetime('now')),
 ('gs_percentage', 'PERCENTAGE', 'Percentage Only',
  'Raw percentage with no band conversion.',
  'percentage', 1, 0, 'active', datetime('now'), datetime('now'));

-- UNEB Primary. Verify against the current UNEB circular before going live;
-- the Custom Band Builder exists so a school can correct any threshold (SRS 4.2).
INSERT INTO grading_bands (id, grading_system_id, label, lower_bound, upper_bound, points, remark, position) VALUES
 ('gb_up_d1', 'gs_uneb_primary', 'D1',  80, 100, 1, 'Distinction',   1),
 ('gb_up_d2', 'gs_uneb_primary', 'D2',  75,  79, 2, 'Distinction',   2),
 ('gb_up_c3', 'gs_uneb_primary', 'C3',  70,  74, 3, 'Credit',        3),
 ('gb_up_c4', 'gs_uneb_primary', 'C4',  65,  69, 4, 'Credit',        4),
 ('gb_up_c5', 'gs_uneb_primary', 'C5',  60,  64, 5, 'Credit',        5),
 ('gb_up_c6', 'gs_uneb_primary', 'C6',  55,  59, 6, 'Credit',        6),
 ('gb_up_p7', 'gs_uneb_primary', 'P7',  50,  54, 7, 'Pass',          7),
 ('gb_up_p8', 'gs_uneb_primary', 'P8',  45,  49, 8, 'Pass',          8),
 ('gb_up_f9', 'gs_uneb_primary', 'F9',   0,  44, 9, 'Fail',          9);

INSERT INTO grading_bands (id, grading_system_id, label, lower_bound, upper_bound, points, remark, position) VALUES
 ('gb_nd_ex', 'gs_nursery_desc', 'Excellent',         80, 100, NULL, 'Excellent work',          1),
 ('gb_nd_vg', 'gs_nursery_desc', 'Very Good',         65,  79, NULL, 'Very good progress',      2),
 ('gb_nd_gd', 'gs_nursery_desc', 'Good',              50,  64, NULL, 'Good progress',           3),
 ('gb_nd_fa', 'gs_nursery_desc', 'Fair',              35,  49, NULL, 'Coming along',            4),
 ('gb_nd_ni', 'gs_nursery_desc', 'Needs Improvement',  0,  34, NULL, 'Needs more support',      5);

INSERT INTO grading_bands (id, grading_system_id, label, lower_bound, upper_bound, points, remark, position) VALUES
 ('gb_pc_all', 'gs_percentage', '%', 0, 100, NULL, NULL, 1);

-- Starter comment bank (FR-G11). Schools add their own; these just mean a
-- Class Teacher is not typing from blank on the first term.
INSERT INTO comment_bank (id, scope, category, text, is_builtin, status, created_at, updated_at) VALUES
 ('cb_ct_01', 'class_teacher', 'Excellent',   'An excellent term''s work. Keep it up.',                                1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_02', 'class_teacher', 'Excellent',   'Consistently strong across all subjects. A pleasure to teach.',         1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_03', 'class_teacher', 'Improving',   'Shows great improvement this term. The effort is showing.',             1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_04', 'class_teacher', 'Improving',   'A much better term. Continue working at this pace.',                    1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_05', 'class_teacher', 'Good',        'A good term''s work. There is still room to do better.',                1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_06', 'class_teacher', 'Effort',      'Needs to work harder on punctuality and class participation.',          1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_07', 'class_teacher', 'Effort',      'Capable of much more. Should concentrate more during lessons.',         1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_08', 'class_teacher', 'Concern',     'Performance has dropped this term. Guardian support is needed.',        1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_09', 'class_teacher', 'Concern',     'Frequent absence has affected this term''s results.',                   1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_10', 'class_teacher', 'Conduct',     'Well behaved, respectful and helpful to others.',                       1, 'active', datetime('now'), datetime('now'));

INSERT INTO comment_bank (id, scope, category, text, is_builtin, status, created_at, updated_at) VALUES
 ('cb_ht_01', 'head_teacher', 'Excellent',    'A commendable performance. Congratulations.',                           1, 'active', datetime('now'), datetime('now')),
 ('cb_ht_02', 'head_teacher', 'Good',         'A satisfactory term. Aim higher next term.',                            1, 'active', datetime('now'), datetime('now')),
 ('cb_ht_03', 'head_teacher', 'Improving',    'Pleasing improvement. Maintain this effort.',                           1, 'active', datetime('now'), datetime('now')),
 ('cb_ht_04', 'head_teacher', 'Effort',       'More effort is required next term.',                                    1, 'active', datetime('now'), datetime('now')),
 ('cb_ht_05', 'head_teacher', 'Concern',      'Please see the Headteacher at the beginning of next term.',             1, 'active', datetime('now'), datetime('now')),
 ('cb_ht_06', 'head_teacher', 'Promotion',    'Promoted to the next class.',                                           1, 'active', datetime('now'), datetime('now'));
"#;

// ---------------------------------------------------------------------------
// 003 — the nursery report card
// ---------------------------------------------------------------------------

const M003_NURSERY_REPORT: &str = r#"

-- The "Behaviour and cleanliness" line on a nursery report.
ALTER TABLE report_comments ADD COLUMN conduct_comment TEXT;

-- How a nursery learner did in each learning activity this term (writing,
-- reading, games, toilet habits ...). The activity list itself is a setting,
-- so a rating is keyed by the activity's name.
CREATE TABLE learning_activity_ratings (
    id         TEXT PRIMARY KEY,
    student_id TEXT NOT NULL REFERENCES students (id),
    term_id    TEXT NOT NULL REFERENCES terms (id),
    activity   TEXT NOT NULL,
    rating     TEXT NOT NULL
                    CHECK (rating IN ('excellent', 'very_good', 'good', 'fair', 'needs_help')),
    updated_by TEXT REFERENCES users (id),
    updated_at TEXT NOT NULL,
    UNIQUE (student_id, term_id, activity)
);

CREATE INDEX idx_activity_ratings_term ON learning_activity_ratings (term_id, student_id);

INSERT INTO comment_bank (id, scope, category, text, is_builtin, status, created_at, updated_at) VALUES
 ('cb_ct_n01', 'class_teacher', 'Nursery', 'Well done. Keep up the good work and aim for consistent excellence in all areas.', 1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_n02', 'class_teacher', 'Nursery', 'A cheerful learner who is growing well. Keep practising at home.',                1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_n03', 'class_teacher', 'Nursery', 'Good progress this term. Needs more support with reading and writing.',            1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_n04', 'class_teacher', 'Conduct', 'Improve on your personal hygiene.',                                                1, 'active', datetime('now'), datetime('now')),
 ('cb_ct_n05', 'class_teacher', 'Conduct', 'Always neat, polite and plays well with others.',                                  1, 'active', datetime('now'), datetime('now'));
"#;

// ---------------------------------------------------------------------------
// 004 — staff photos, for staff ID cards (FR-G16)
// ---------------------------------------------------------------------------

const M004_STAFF_PHOTOS: &str = r#"

ALTER TABLE users ADD COLUMN photo_png BLOB;
"#;

// ---------------------------------------------------------------------------
// 005 — pass-outs (FR-G22) and the SMS outbox (FR-G8)
// ---------------------------------------------------------------------------

const M005_PASS_OUTS_AND_SMS: &str = r#"

-- Every message RM sends, written here first and sent when there is internet.
CREATE TABLE sms_outbox (
    id           TEXT PRIMARY KEY,
    to_phone     TEXT NOT NULL,             -- +256XXXXXXXXX
    body         TEXT NOT NULL,
    kind         TEXT NOT NULL,             -- pass_out, pass_out_return, test
    related_id   TEXT,
    status       TEXT NOT NULL DEFAULT 'queued'
                      CHECK (status IN ('queued', 'sent', 'failed')),
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    provider_ref TEXT,
    created_by   TEXT REFERENCES users (id),
    created_at   TEXT NOT NULL,
    sent_at      TEXT
);

CREATE INDEX idx_sms_outbox_status ON sms_outbox (status, created_at);

-- A learner leaving the school during the day (FR-G22). Times are UTC,
-- RFC 3339 to the second, so they sort and compare as text.
CREATE TABLE pass_outs (
    id                     TEXT PRIMARY KEY,
    number                 INTEGER NOT NULL UNIQUE,   -- printed as PO-0001
    student_id             TEXT NOT NULL REFERENCES students (id),
    class_id               TEXT NOT NULL REFERENCES classes (id),
    reason_kind            TEXT NOT NULL
                                CHECK (reason_kind IN ('sick', 'appointment', 'family', 'permission', 'other')),
    reason                 TEXT,
    destination            TEXT,
    picked_up_by           TEXT,
    picked_up_relationship TEXT,
    picked_up_phone        TEXT,
    time_out               TEXT NOT NULL,
    -- NULL means the learner is not coming back today.
    expected_back          TEXT,
    returned_at            TEXT,
    status                 TEXT NOT NULL DEFAULT 'out'
                                CHECK (status IN ('out', 'returned')),
    guardian_phone         TEXT,
    sms_out_id             TEXT REFERENCES sms_outbox (id),
    sms_return_id          TEXT REFERENCES sms_outbox (id),
    issued_by              TEXT REFERENCES users (id),
    returned_by            TEXT REFERENCES users (id),
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL
);

CREATE INDEX idx_pass_outs_time ON pass_outs (time_out DESC);
CREATE INDEX idx_pass_outs_status ON pass_outs (status, expected_back);
"#;

// ---------------------------------------------------------------------------
// 006 — weekly assignments (FR-C12)
// ---------------------------------------------------------------------------

const M006_WEEKLY_ASSIGNMENTS: &str = r#"

-- One learner's score on one subject's assignment for one week of a term.
-- `out_of` is kept per row: a teacher may mark one week out of 10 and the
-- next out of 20, and the percentage must stay right for both.
CREATE TABLE weekly_scores (
    id               TEXT PRIMARY KEY,
    student_id       TEXT NOT NULL REFERENCES students (id),
    class_subject_id TEXT NOT NULL REFERENCES class_subjects (id),
    term_id          TEXT NOT NULL REFERENCES terms (id),
    week             INTEGER NOT NULL CHECK (week BETWEEN 1 AND 20),
    score            REAL,
    out_of           REAL NOT NULL DEFAULT 10 CHECK (out_of > 0),
    remark           TEXT,
    entered_by       TEXT REFERENCES users (id),
    updated_at       TEXT NOT NULL,
    CHECK (score IS NULL OR (score >= 0 AND score <= out_of)),
    UNIQUE (student_id, class_subject_id, term_id, week)
);

CREATE INDEX idx_weekly_scores_sheet ON weekly_scores (class_subject_id, term_id, week);
CREATE INDEX idx_weekly_scores_student ON weekly_scores (student_id, term_id);
"#;

// ---------------------------------------------------------------------------
// 007 — class timetables (FR-G6) and exam timetables (FR-G7)
// ---------------------------------------------------------------------------

const M007_TIMETABLES: &str = r#"

-- The school day: lessons, breaks, lunch, assembly, in order.
CREATE TABLE timetable_periods (
    id         TEXT PRIMARY KEY,
    seq        INTEGER NOT NULL,
    label      TEXT NOT NULL,
    start_time TEXT NOT NULL,              -- HH:MM
    end_time   TEXT NOT NULL,              -- HH:MM
    kind       TEXT NOT NULL DEFAULT 'lesson'
                    CHECK (kind IN ('lesson', 'break', 'lunch', 'assembly', 'games', 'prep')),
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- One lesson in a class's week. A NULL stream is the whole class.
CREATE TABLE timetable_slots (
    id         TEXT PRIMARY KEY,
    class_id   TEXT NOT NULL REFERENCES classes (id),
    stream_id  TEXT REFERENCES streams (id),
    day        INTEGER NOT NULL CHECK (day BETWEEN 1 AND 6),   -- 1 = Monday
    period_id  TEXT NOT NULL REFERENCES timetable_periods (id),
    subject_id TEXT REFERENCES subjects (id),
    teacher_id TEXT REFERENCES users (id),
    room       TEXT,
    note       TEXT,
    updated_by TEXT REFERENCES users (id),
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_timetable_slot_cell
    ON timetable_slots (class_id, IFNULL(stream_id, '-'), day, period_id);
CREATE INDEX idx_timetable_slot_teacher ON timetable_slots (teacher_id, day, period_id);

-- How many lessons a subject gets each week, for the auto-fill.
ALTER TABLE class_subjects ADD COLUMN lessons_per_week INTEGER NOT NULL DEFAULT 0;

-- One sitting of an examination: a subject's paper on a date and time,
-- sat by one or more classes.
CREATE TABLE exam_papers (
    id            TEXT PRIMARY KEY,
    exam_id       TEXT NOT NULL REFERENCES exams (id),
    subject_id    TEXT NOT NULL REFERENCES subjects (id),
    paper_label   TEXT,                    -- "Paper 1", "Oral", ...
    on_date       TEXT NOT NULL,           -- YYYY-MM-DD
    start_time    TEXT NOT NULL,           -- HH:MM
    end_time      TEXT NOT NULL,           -- HH:MM
    venue         TEXT,
    invigilator_id TEXT REFERENCES users (id),
    note          TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    CHECK (end_time > start_time)
);

CREATE TABLE exam_paper_classes (
    paper_id TEXT NOT NULL REFERENCES exam_papers (id) ON DELETE CASCADE,
    class_id TEXT NOT NULL REFERENCES classes (id),
    PRIMARY KEY (paper_id, class_id)
);

CREATE INDEX idx_exam_papers_exam ON exam_papers (exam_id, on_date, start_time);

-- A sensible default school day, editable under Timetables.
INSERT INTO timetable_periods (id, seq, label, start_time, end_time, kind, created_at, updated_at) VALUES
 ('tp_01', 1,  'Assembly',  '08:00', '08:20', 'assembly', datetime('now'), datetime('now')),
 ('tp_02', 2,  'Lesson 1',  '08:20', '09:00', 'lesson',   datetime('now'), datetime('now')),
 ('tp_03', 3,  'Lesson 2',  '09:00', '09:40', 'lesson',   datetime('now'), datetime('now')),
 ('tp_04', 4,  'Lesson 3',  '09:40', '10:20', 'lesson',   datetime('now'), datetime('now')),
 ('tp_05', 5,  'Break',     '10:20', '10:50', 'break',    datetime('now'), datetime('now')),
 ('tp_06', 6,  'Lesson 4',  '10:50', '11:30', 'lesson',   datetime('now'), datetime('now')),
 ('tp_07', 7,  'Lesson 5',  '11:30', '12:10', 'lesson',   datetime('now'), datetime('now')),
 ('tp_08', 8,  'Lesson 6',  '12:10', '12:50', 'lesson',   datetime('now'), datetime('now')),
 ('tp_09', 9,  'Lunch',     '12:50', '14:00', 'lunch',    datetime('now'), datetime('now')),
 ('tp_10', 10, 'Lesson 7',  '14:00', '14:40', 'lesson',   datetime('now'), datetime('now')),
 ('tp_11', 11, 'Lesson 8',  '14:40', '15:20', 'lesson',   datetime('now'), datetime('now')),
 ('tp_12', 12, 'Games',     '15:20', '16:30', 'games',    datetime('now'), datetime('now'));
"#;
