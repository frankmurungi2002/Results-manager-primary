#!/usr/bin/env python3
"""
Verify the database layer without compiling anything.

Three checks, in order of how expensive the bug would be if it escaped:

  1. The migrations produce a valid schema.
  2. Every SQL statement in the Rust compiles against that schema. A wrong
     column name compiles fine in Rust and fails in front of a headteacher;
     this catches it in a second.
  3. The schema's guarantees actually hold — the append-only audit log, the
     FR-B6 teacher caps, unique registration numbers, and the rule that an
     absence, a blank and a zero stay three different things.
  4. Every shape Rust sends matches the TypeScript that receives it. Neither
     compiler can see across the IPC boundary: Rust does not know what the
     interface expects, and TypeScript trusts a hand-written type. A struct
     missing `rename_all = "camelCase"` makes every field `undefined` on the
     other side, which blanks the screen.

    python3 scripts/verify.py

Needs only Python 3 and its bundled sqlite3. Run it after any change to
migrations.rs or to a query, before reaching for `cargo build`.
"""

import glob
import os
import re
import sqlite3
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MIGRATIONS = os.path.join(ROOT, "src-tauri", "src", "db", "migrations.rs")

GREEN, RED, DIM, RESET = "\033[32m", "\033[31m", "\033[2m", "\033[0m"
if not sys.stdout.isatty() or os.name == "nt":
    GREEN = RED = DIM = RESET = ""


def build_schema():
    """Check 1 — run every migration block into a fresh in-memory database."""
    src = open(MIGRATIONS, encoding="utf-8").read()
    blocks = re.findall(r'const (M\d+_[A-Z_]+): &str = r#"(.*?)"#;', src, re.S)
    if not blocks:
        raise SystemExit("no migration blocks found in migrations.rs")

    con = sqlite3.connect(":memory:")
    con.execute("PRAGMA foreign_keys = ON")
    # The runner creates this before applying anything.
    con.execute(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY,"
        " name TEXT NOT NULL, applied_at TEXT NOT NULL)"
    )

    for name, sql in blocks:
        con.executescript(sql)
        print(f"  {GREEN}ok{RESET}  {name}")

    con.execute("PRAGMA foreign_keys = ON")  # executescript commits
    counts = {
        kind: con.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = ?", (kind,)
        ).fetchone()[0]
        for kind in ("table", "index", "trigger", "view")
    }
    print(
        f"  {DIM}{counts['table']} tables, {counts['index']} indexes, "
        f"{counts['trigger']} triggers, {counts['view']} view{RESET}"
    )
    return con, len(blocks)


def check_queries(con):
    """Check 2 — EXPLAIN every SQL literal in the Rust against the schema."""
    literal = re.compile(r'"((?:[^"\\]|\\.)*)"', re.S)
    # Full clause, not a bare keyword — otherwise an `.expect("insert")` in a
    # test gets mistaken for a query.
    sqlish = re.compile(
        r"^\s*(SELECT\s|INSERT\s+INTO\s|UPDATE\s+\w+\s+SET\s|DELETE\s+FROM\s"
        r"|WITH\s+RECURSIVE\s|REPLACE\s+INTO\s)",
        re.I | re.S,
    )

    files = sorted(
        glob.glob(os.path.join(ROOT, "src-tauri", "src", "commands", "*.rs"))
        + glob.glob(os.path.join(ROOT, "src-tauri", "src", "*.rs"))
        + glob.glob(os.path.join(ROOT, "src-tauri", "src", "domain", "*.rs"))
        + glob.glob(os.path.join(ROOT, "src-tauri", "src", "db", "*.rs"))
    )

    total = 0
    problems = []
    for path in files:
        text = open(path, encoding="utf-8").read()
        for match in literal.finditer(text):
            raw = match.group(1)
            if not sqlish.match(raw):
                continue
            try:
                sql = raw.encode().decode("unicode_escape")
            except UnicodeDecodeError:
                continue
            total += 1
            line = text[: match.start()].count("\n") + 1
            try:
                # EXPLAIN compiles without executing. A binding-count error is
                # raised after a successful compile, so it means the SQL is fine.
                con.execute("EXPLAIN " + sql)
            except sqlite3.ProgrammingError:
                pass
            except sqlite3.OperationalError as error:
                problems.append((os.path.relpath(path, ROOT), line, str(error), sql))

    for path, line, error, sql in problems:
        print(f"  {RED}FAIL{RESET}  {path}:{line} — {error}")
        print(f"        {DIM}{' '.join(sql.split())[:160]}{RESET}")

    print(f"  {GREEN if not problems else RED}{total - len(problems)}/{total}{RESET}"
          f" statements compile")
    return len(problems)


def check_invariants(con):
    """Check 3 — attack the schema and confirm it refuses what it should."""
    now = "2026-01-01T00:00:00Z"
    failures = []

    def case(name, sql, params=(), must_fail=False):
        try:
            con.execute(sql, params)
            if must_fail:
                failures.append(f"{name} — was ALLOWED but must be refused")
                print(f"  {RED}FAIL{RESET}  {name}")
            else:
                print(f"  {GREEN}ok{RESET}  {name}")
        except sqlite3.Error as error:
            if must_fail:
                print(f"  {GREEN}ok{RESET}  {name}")
            else:
                failures.append(f"{name} — {error}")
                print(f"  {RED}FAIL{RESET}  {name}: {error}")

    con.execute(
        "INSERT INTO institution (id,name,accent_color,reg_number_pattern,"
        "reg_number_next,created_at,updated_at) VALUES (1,'Test','#4F63D2',"
        "'{YEAR}/{SEQ:4}',1,?,?)", (now, now))
    for uid in ("u1", "u2", "u3"):
        con.execute(
            "INSERT INTO users (id,username,full_name,role,password_hash,"
            "created_at,updated_at) VALUES (?,?,?,'teacher','$x',?,?)",
            (uid, uid, uid, now, now))
    con.execute(
        "INSERT INTO classes (id,code,name,level_kind,ladder_position,"
        "default_grading_system_id,created_at,updated_at) VALUES "
        "('c1','P5','P5','primary',8,'gs_uneb_primary',?,?)", (now, now))
    con.execute("INSERT INTO academic_years (id,label,status,created_at,updated_at)"
                " VALUES ('y1','2026','active',?,?)", (now, now))
    con.execute("INSERT INTO terms (id,academic_year_id,seq,name,status,created_at,"
                "updated_at) VALUES ('t1','y1',1,'Term 1','open',?,?)", (now, now))
    con.execute("INSERT INTO exams (id,term_id,seq,code,name,status,created_at,"
                "updated_at) VALUES ('e1','t1',1,'EOT','End of Term','open',?,?)",
                (now, now))
    con.execute("INSERT INTO subjects (id,code,name,created_at,updated_at)"
                " VALUES ('s1','ENG','English',?,?)", (now, now))
    con.execute("INSERT INTO class_subjects (id,class_id,subject_id,position,"
                "created_at,updated_at) VALUES ('cs1','c1','s1',1,?,?)", (now, now))
    con.execute("INSERT INTO students (id,reg_number,full_name,created_at,updated_at)"
                " VALUES ('st1','2026/0001','A Learner',?,?)", (now, now))
    con.execute("INSERT INTO enrollments (id,student_id,class_id,academic_year_id,"
                "status,joined_at,created_at,updated_at) VALUES ('en1','st1','c1',"
                "'y1','active',?,?,?)", (now, now, now))
    con.execute("INSERT INTO audit_log (at,action,summary) VALUES (?,'seed','x')", (now,))
    con.commit()

    # FR-E2 — the record of what happened cannot be rewritten.
    case("audit_log refuses UPDATE", "UPDATE audit_log SET summary='tampered'",
         must_fail=True)
    case("audit_log refuses DELETE", "DELETE FROM audit_log", must_fail=True)

    # FR-B6 — one Class Teacher, at most one Assistant, one teacher per subject.
    con.execute("INSERT INTO teacher_assignments (id,user_id,class_id,role,"
                "created_at,updated_at) VALUES ('a1','u1','c1','class_teacher',?,?)",
                (now, now))
    case("FR-B6 refuses a second Class Teacher",
         "INSERT INTO teacher_assignments (id,user_id,class_id,role,created_at,"
         "updated_at) VALUES ('a2','u2','c1','class_teacher',?,?)", (now, now),
         must_fail=True)
    case("FR-B6 allows one Assistant",
         "INSERT INTO teacher_assignments (id,user_id,class_id,role,created_at,"
         "updated_at) VALUES ('a3','u2','c1','assistant_class_teacher',?,?)",
         (now, now))
    case("FR-B6 refuses a second Assistant",
         "INSERT INTO teacher_assignments (id,user_id,class_id,role,created_at,"
         "updated_at) VALUES ('a4','u3','c1','assistant_class_teacher',?,?)",
         (now, now), must_fail=True)

    # FR-B5 — registration numbers are unique institution-wide.
    case("FR-B5 refuses a duplicate registration number",
         "INSERT INTO students (id,reg_number,full_name,created_at,updated_at)"
         " VALUES ('stX','2026/0001','Clone',?,?)", (now, now), must_fail=True)

    # FR-C10 — one enrollment per learner per year.
    case("refuses two enrollments for one learner in one year",
         "INSERT INTO enrollments (id,student_id,class_id,academic_year_id,status,"
         "joined_at,created_at,updated_at) VALUES ('enX','st1','c1','y1','active',"
         "?,?,?)", (now, now, now), must_fail=True)

    # FR-C3 — a blank, an absence and a zero are three different things.
    case("refuses a mark that is neither a score nor an absence",
         "INSERT INTO marks (id,student_id,class_subject_id,exam_id,score,is_absent,"
         "entered_at,updated_at) VALUES ('mX','st1','cs1','e1',NULL,0,?,?)",
         (now, now), must_fail=True)
    case("accepts an absence with no score",
         "INSERT INTO marks (id,student_id,class_subject_id,exam_id,score,is_absent,"
         "entered_at,updated_at) VALUES ('mY','st1','cs1','e1',NULL,1,?,?)",
         (now, now))
    case("refuses two marks for one learner, subject and exam",
         "INSERT INTO marks (id,student_id,class_subject_id,exam_id,score,is_absent,"
         "entered_at,updated_at) VALUES ('mZ','st1','cs1','e1',50,0,?,?)",
         (now, now), must_fail=True)
    case("refuses a mark for a learner who does not exist",
         "INSERT INTO marks (id,student_id,class_subject_id,exam_id,score,is_absent,"
         "entered_at,updated_at) VALUES ('mW','ghost','cs1','e1',50,0,?,?)",
         (now, now), must_fail=True)

    # SRS 4.2 — every possible mark must land in exactly one band.
    for system in ("gs_uneb_primary", "gs_nursery_desc", "gs_percentage"):
        uncovered = con.execute(
            "SELECT COUNT(*) FROM (WITH RECURSIVE c(n) AS (SELECT 0 UNION ALL"
            " SELECT n+1 FROM c WHERE n<100) SELECT n FROM c) v"
            " WHERE (SELECT COUNT(*) FROM grading_bands WHERE grading_system_id = ?"
            "        AND v.n BETWEEN lower_bound AND upper_bound) != 1",
            (system,)).fetchone()[0]
        if uncovered:
            failures.append(f"{system} leaves {uncovered} percentages uncovered")
            print(f"  {RED}FAIL{RESET}  {system}: {uncovered} percentages "
                  f"uncovered or double-covered")
        else:
            print(f"  {GREEN}ok{RESET}  {system}: 0-100 covered exactly once")

    return failures



def block(src, open_index):
    """Return the {...} body starting at the brace at open_index."""
    depth, i = 0, open_index
    while i < len(src):
        if src[i] == "{": depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0: return src[open_index + 1 : i], i
        i += 1
    return "", len(src)

def camel(name):
    head, *rest = name.split("_")
    return head + "".join(p[:1].upper() + p[1:] for p in rest)

def rust_structs(root):
    out = {}
    struct_re = re.compile(r'#\[derive\((?P<d>[^)]*)\)\]\s*(?P<attrs>(?:#\[[^\]]*\]\s*)*)pub struct (?P<name>\w+)(?:<[^>]*>)?\s*\{')
    for path in sorted(glob.glob(os.path.join(root, "**/*.rs"), recursive=True)):
        src = open(path, encoding="utf-8").read()
        for m in struct_re.finditer(src):
            if "Serialize" not in m.group("d") and "Deserialize" not in m.group("d"):
                continue
            body, _ = block(src, m.end() - 1)
            renames = 'rename_all = "camelCase"' in m.group("attrs")

            fields, skip_next, rename_next = [], False, None
            for line in body.split("\n"):
                s = line.strip()
                if s.startswith("#["):
                    if "skip" in s: skip_next = True
                    r = re.search(r'rename\s*=\s*"([^"]+)"', s)
                    if r: rename_next = r.group(1)
                    continue
                fm = re.match(r'pub (\w+)\s*:', s)
                if not fm:
                    continue
                if skip_next:
                    skip_next, rename_next = False, None
                    continue
                name = fm.group(1)
                wire = rename_next or (camel(name) if renames else name)
                fields.append(wire)
                skip_next, rename_next = False, None

            if fields:
                out[m.group("name")] = (os.path.relpath(path), fields)
    return out

def ts_interfaces(path):
    src = open(path, encoding="utf-8").read()
    out = {}
    for m in re.finditer(r'export interface (\w+)(?:<[^>]*>)?\s*\{', src):
        body, _ = block(src, m.end() - 1)
        fields = []
        for line in body.split("\n"):
            s = line.strip()
            if not s or s.startswith("//") or s.startswith("*") or s.startswith("/*"):
                continue
            fm = re.match(r'(\w+)\??\s*:', s)
            if fm: fields.append(fm.group(1))
        out[m.group(1)] = fields
    return out


ALIASES = {"Band": "GradingBand"}
# Shapes that never cross the IPC boundary, so their field names are free.
INTERNAL = {"Session", "GradedMark", "Aggregate", "BandGap", "ExamSlot", "MarkEntry"}


def check_ipc_shapes():
    """Check 4 — what Rust sends is what TypeScript expects."""
    rust = rust_structs(os.path.join(ROOT, "src-tauri", "src"))
    ts = ts_interfaces(os.path.join(ROOT, "src", "lib", "types.ts"))

    problems, checked = [], 0
    for name in sorted(rust):
        if name in INTERNAL:
            continue
        target = ALIASES.get(name, name)
        if target not in ts:
            continue
        checked += 1
        path, rust_fields = rust[name]
        ts_fields = ts[target]
        only_rust = [f for f in rust_fields if f not in ts_fields]
        only_ts = [f for f in ts_fields if f not in rust_fields]
        if only_rust or only_ts:
            problems.append(name)
            print(f"  {RED}FAIL{RESET}  {name} ({path}) vs TypeScript {target}")
            if only_rust:
                print(f"        {DIM}Rust sends, TS does not expect: "
                      f"{', '.join(only_rust)}{RESET}")
            if only_ts:
                print(f"        {DIM}TS expects, Rust does not send: "
                      f"{', '.join(only_ts)}{RESET}")

    colour = GREEN if not problems else RED
    print(f"  {colour}{checked - len(problems)}/{checked}{RESET} shapes agree")
    return len(problems)


def main():
    print("\nSchema")
    con, blocks = build_schema()

    print("\nQueries")
    bad_queries = check_queries(con)

    print("\nGuarantees")
    failures = check_invariants(con)

    print("\nIPC shapes")
    bad_shapes = check_ipc_shapes()

    print()
    if bad_queries or failures or bad_shapes:
        print(f"{RED}FAILED{RESET} — {bad_queries} bad quer"
              f"{'y' if bad_queries == 1 else 'ies'}, "
              f"{len(failures)} broken guarantee(s), "
              f"{bad_shapes} shape mismatch(es)")
        return 1

    print(f"{GREEN}All checks passed{RESET} "
          f"({blocks} migrations, schema valid, queries compile, "
          f"guarantees hold, IPC shapes agree)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
