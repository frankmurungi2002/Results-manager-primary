/**
 * FR-C3 — marks entry.
 *
 * Two of the three methods live here: the in-app spreadsheet grid and the
 * guided one-learner-at-a-time form. SRS 14.7 requires them to be equally
 * reliable, so they are two views over one piece of state and one save path —
 * switching between them mid-entry keeps everything typed so far.
 *
 * FR-C5 (subject analytics) and FR-C6 (deadline and progress) sit alongside.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  BarChart3,
  Check,
  ClipboardList,
  Save,
  UserX,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  AcademicYearRow,
  ClassRow,
  ClassSubjectRow,
  ExamRow,
  MarkEntry,
  MarksSheet,
  SubjectAnalytics,
  SubjectProgress,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Loading,
  Modal,
  Progress,
  SelectInput,
  Segmented,
  TableSkeleton,
  cx,
} from "../components/ui";

type Draft = Record<string, { score: string; isAbsent: boolean }>;

export function MarksScreen() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [classId, setClassId] = useState("");
  const [examId, setExamId] = useState("");
  const [subjects, setSubjects] = useState<ClassSubjectRow[]>([]);
  const [classSubjectId, setClassSubjectId] = useState("");

  const [sheet, setSheet] = useState<MarksSheet | null>(null);
  const [draft, setDraft] = useState<Draft>({});
  const [loadingSheet, setLoadingSheet] = useState(false);
  const [saving, setSaving] = useState(false);
  const [mode, setMode] = useState<"grid" | "form">("grid");
  const [formIndex, setFormIndex] = useState(0);
  const [progress, setProgress] = useState<SubjectProgress[]>([]);
  const [analytics, setAnalytics] = useState<SubjectAnalytics | null>(null);
  const [bootstrapping, setBootstrapping] = useState(true);

  // --- Load the pickers ---------------------------------------------------

  useEffect(() => {
    Promise.all([api.listClasses(), api.listAcademicYears()])
      .then(([loadedClasses, loadedYears]) => {
        setClasses(loadedClasses);
        setYears(loadedYears);

        if (loadedClasses.length > 0) setClassId(loadedClasses[0]!.id);

        // Default to the open term's first examination — the one a teacher is
        // overwhelmingly likely to want.
        const openTerm = loadedYears
          .flatMap((year) => year.terms)
          .find((term) => term.status === "open");
        const firstExam = openTerm?.exams[0];
        if (firstExam) setExamId(firstExam.id);
      })
      .catch(reportError)
      .finally(() => setBootstrapping(false));
  }, [reportError]);

  useEffect(() => {
    if (!classId) return;
    api
      .listClassSubjects(classId)
      .then((loaded) => {
        setSubjects(loaded);
        setClassSubjectId((current) =>
          loaded.some((subject) => subject.id === current)
            ? current
            : (loaded[0]?.id ?? ""),
        );
      })
      .catch(reportError);
  }, [classId, reportError]);

  const exams: ExamRow[] = useMemo(
    () => years.flatMap((year) => year.terms).flatMap((term) => term.exams),
    [years],
  );

  const currentExam = exams.find((exam) => exam.id === examId);

  // --- Load the sheet -----------------------------------------------------

  const loadSheet = useCallback(async () => {
    if (!classSubjectId || !examId) {
      setSheet(null);
      return;
    }
    setLoadingSheet(true);
    try {
      const loaded = await api.loadMarksSheet(classSubjectId, examId);
      setSheet(loaded);
      setDraft(
        Object.fromEntries(
          loaded.rows.map((row) => [
            row.studentId,
            { score: row.score === null ? "" : String(row.score), isAbsent: row.isAbsent },
          ]),
        ),
      );
      setFormIndex(0);
    } catch (error) {
      reportError(error);
      setSheet(null);
    } finally {
      setLoadingSheet(false);
    }
  }, [classSubjectId, examId, reportError]);

  useEffect(() => {
    void loadSheet();
  }, [loadSheet]);

  useEffect(() => {
    if (!classId || !examId) return;
    api.marksProgress(classId, examId).then(setProgress).catch(() => undefined);
  }, [classId, examId, sheet]);

  // --- Dirty tracking -----------------------------------------------------

  const dirty = useMemo(() => {
    if (!sheet) return false;
    return sheet.rows.some((row) => {
      const entry = draft[row.studentId];
      if (!entry) return false;
      const original = row.score === null ? "" : String(row.score);
      return entry.score !== original || entry.isAbsent !== row.isAbsent;
    });
  }, [sheet, draft]);

  // Warn before the window closes on unsaved marks.
  useEffect(() => {
    if (!dirty) return;
    function onBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [dirty]);

  // Ctrl+S saves, because that is what everyone tries.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (dirty && !saving) void save();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  async function save() {
    if (!sheet) return;
    setSaving(true);
    try {
      const entries: MarkEntry[] = sheet.rows.map((row) => {
        const entry = draft[row.studentId]!;
        const trimmed = entry.score.trim();
        return {
          studentId: row.studentId,
          score: entry.isAbsent || trimmed === "" ? null : Number(trimmed),
          isAbsent: entry.isAbsent,
        };
      });

      const result = await api.saveMarks(sheet.classSubjectId, sheet.examId, entries);

      if (result.skipped.length > 0) {
        toast(
          "warning",
          `Saved ${result.saved}. ${result.skipped.length} could not be saved: ${result.skipped[0]!.studentName} — ${result.skipped[0]!.reason}`,
        );
      } else {
        toast("success", `Saved ${result.saved} marks.`);
      }

      await loadSheet();
    } catch (error) {
      reportError(error);
    } finally {
      setSaving(false);
    }
  }

  function setScore(studentId: string, score: string) {
    setDraft((current) => ({
      ...current,
      [studentId]: { score, isAbsent: false },
    }));
  }

  function toggleAbsent(studentId: string) {
    setDraft((current) => {
      const entry = current[studentId] ?? { score: "", isAbsent: false };
      return {
        ...current,
        [studentId]: { score: entry.isAbsent ? entry.score : "", isAbsent: !entry.isAbsent },
      };
    });
  }

  if (bootstrapping) {
    return (
      <div className="page">
        <Loading label="Loading your classes" />
      </div>
    );
  }

  if (classes.length === 0) {
    return (
      <div className="page">
        <div className="page-inner">
          <Card>
            <EmptyState icon={<ClipboardList size={18} />} title="No classes yet">
              A School Admin needs to create classes before marks can be entered.
            </EmptyState>
          </Card>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Marks entry</h1>
            <p className="page-description">
              Type marks straight into the grid, or step through one learner at
              a time. The grade updates as you type, using this subject's own
              grading system.
            </p>
          </div>
          <div className="page-actions">
            <Segmented
              value={mode}
              onChange={setMode}
              options={[
                { value: "grid", label: "Grid" },
                { value: "form", label: "One by one" },
              ]}
            />
            {sheet && (
              <Button
                variant="ghost"
                icon={<BarChart3 size={15} />}
                onClick={() =>
                  api
                    .subjectAnalytics(sheet.classSubjectId, sheet.examId)
                    .then(setAnalytics)
                    .catch(reportError)
                }
              >
                Analytics
              </Button>
            )}
            <Button
              variant="primary"
              icon={<Save size={15} />}
              loading={saving}
              disabled={!dirty || !sheet?.isOpen}
              onClick={() => void save()}
            >
              {dirty ? "Save marks" : "Saved"}
            </Button>
          </div>
        </div>

        <Card>
          <div className="grid-form">
            <SelectInput
              label="Class"
              value={classId}
              onChange={(event) => setClassId(event.target.value)}
            >
              {classes.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.name} ({entry.learnerCount} learners)
                </option>
              ))}
            </SelectInput>

            <SelectInput
              label="Subject"
              value={classSubjectId}
              onChange={(event) => setClassSubjectId(event.target.value)}
            >
              {subjects.length === 0 && <option value="">No subjects</option>}
              {subjects.map((subject) => (
                <option key={subject.id} value={subject.id}>
                  {subject.displayName}
                  {subject.isCore ? "" : " (not aggregated)"}
                </option>
              ))}
            </SelectInput>

            <SelectInput
              label="Examination"
              value={examId}
              onChange={(event) => setExamId(event.target.value)}
            >
              {years.map((year) =>
                year.terms.map((term) => (
                  <optgroup key={term.id} label={`${year.label} · ${term.name}`}>
                    {term.exams.map((exam) => (
                      <option key={exam.id} value={exam.id}>
                        {exam.name}
                        {exam.status === "open" ? "" : " — closed"}
                      </option>
                    ))}
                  </optgroup>
                )),
              )}
            </SelectInput>
          </div>
        </Card>

        {sheet && !sheet.isOpen && (
          <Alert tone="warning" title="This examination is closed">
            Marks can be read but not changed. A School Admin can reopen the
            term from Terms &amp; exams.
          </Alert>
        )}

        {loadingSheet ? (
          <Card flush>
            <TableSkeleton rows={8} columns={4} />
          </Card>
        ) : !sheet ? (
          <Card>
            <EmptyState icon={<ClipboardList size={18} />} title="Choose a subject">
              Pick a class, a subject and an examination to start entering marks.
            </EmptyState>
          </Card>
        ) : sheet.rows.length === 0 ? (
          <Card>
            <EmptyState icon={<ClipboardList size={18} />} title="No learners in this class">
              Add learners to {sheet.className} before entering marks.
            </EmptyState>
          </Card>
        ) : mode === "grid" ? (
          <MarksGrid
            sheet={sheet}
            draft={draft}
            onScore={setScore}
            onToggleAbsent={toggleAbsent}
          />
        ) : (
          <GuidedForm
            sheet={sheet}
            draft={draft}
            index={formIndex}
            onIndex={setFormIndex}
            onScore={setScore}
            onToggleAbsent={toggleAbsent}
          />
        )}

        {progress.length > 0 && (
          <Card
            title="Marks entered this examination"
            subtitle={`${sheet?.className ?? ""}${currentExam ? ` · ${currentExam.name}` : ""}`}
            flush
          >
            <div className="table-wrap">
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Subject</th>
                    <th>Teacher</th>
                    <th style={{ width: 180 }}>Progress</th>
                    <th className="num" style={{ width: 100 }}>
                      Entered
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {progress.map((row) => (
                    <tr key={row.classSubjectId}>
                      <td>{row.subjectName}</td>
                      <td className="muted">{row.teacherName ?? "Not assigned"}</td>
                      <td>
                        <Progress value={row.percent} />
                      </td>
                      <td className="num">
                        {row.entered} / {row.expected}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Card>
        )}
      </div>

      <Modal
        open={analytics !== null}
        title={analytics ? `${analytics.subjectName} — ${analytics.examName}` : ""}
        onClose={() => setAnalytics(null)}
        wide
      >
        {analytics && <AnalyticsPanel analytics={analytics} />}
      </Modal>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The grid
// ---------------------------------------------------------------------------

function MarksGrid({
  sheet,
  draft,
  onScore,
  onToggleAbsent,
}: {
  sheet: MarksSheet;
  draft: Draft;
  onScore: (studentId: string, score: string) => void;
  onToggleAbsent: (studentId: string) => void;
}) {
  const inputs = useRef<(HTMLInputElement | null)[]>([]);

  /** Enter and the arrow keys move down the column, the way a mark sheet is read. */
  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>, index: number) {
    if (event.key === "Enter" || event.key === "ArrowDown") {
      event.preventDefault();
      inputs.current[index + 1]?.focus();
      inputs.current[index + 1]?.select();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      inputs.current[index - 1]?.focus();
      inputs.current[index - 1]?.select();
    }
  }

  const entered = sheet.rows.filter((row) => {
    const entry = draft[row.studentId];
    return entry && (entry.score.trim() !== "" || entry.isAbsent);
  }).length;

  return (
    <Card
      title={`${sheet.subjectName} — ${sheet.examName}`}
      subtitle={`${sheet.className} · out of ${sheet.maxScore} · graded on ${sheet.gradingSystemName}`}
      actions={
        <Badge tone={entered === sheet.rows.length ? "success" : "neutral"}>
          {entered} of {sheet.rows.length} entered
        </Badge>
      }
      flush
    >
      <div className="table-wrap" style={{ maxHeight: "58vh" }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 48 }} className="num">
                #
              </th>
              <th style={{ width: 150 }}>Reg. No.</th>
              <th>Name</th>
              <th style={{ width: 130 }} className="num">
                Mark / {sheet.maxScore}
              </th>
              <th style={{ width: 90 }} className="center">
                Grade
              </th>
              <th style={{ width: 90 }} className="center">
                Absent
              </th>
            </tr>
          </thead>
          <tbody>
            {sheet.rows.map((row, index) => {
              const entry = draft[row.studentId] ?? { score: "", isAbsent: false };
              const numeric = Number(entry.score);
              const invalid =
                entry.score.trim() !== "" &&
                (Number.isNaN(numeric) || numeric < 0 || numeric > sheet.maxScore);

              return (
                <tr key={row.studentId}>
                  <td className="num subtle">{index + 1}</td>
                  <td className="mono muted">{row.regNumber}</td>
                  <td>{row.fullName}</td>
                  <td className="num">
                    <input
                      ref={(element) => {
                        inputs.current[index] = element;
                      }}
                      className={cx("input", invalid && "is-invalid")}
                      style={{ textAlign: "right", height: 30 }}
                      inputMode="decimal"
                      value={entry.isAbsent ? "" : entry.score}
                      disabled={entry.isAbsent || !sheet.isOpen}
                      placeholder={entry.isAbsent ? "—" : ""}
                      onChange={(event) => onScore(row.studentId, event.target.value)}
                      onKeyDown={(event) => onKeyDown(event, index)}
                      onFocus={(event) => event.target.select()}
                    />
                  </td>
                  <td className="center">
                    <GradePreview
                      sheet={sheet}
                      score={entry.score}
                      isAbsent={entry.isAbsent}
                      invalid={invalid}
                    />
                  </td>
                  <td className="center">
                    <input
                      type="checkbox"
                      checked={entry.isAbsent}
                      disabled={!sheet.isOpen}
                      onChange={() => onToggleAbsent(row.studentId)}
                      aria-label={`Mark ${row.fullName} absent`}
                    />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div
        className="row"
        style={{
          padding: "var(--space-3) var(--space-4)",
          borderTop: "1px solid var(--border-subtle)",
          fontSize: "var(--text-xs)",
          color: "var(--text-tertiary)",
        }}
      >
        <span>
          Press <kbd>Enter</kbd> to move to the next learner. <kbd>Ctrl</kbd>+
          <kbd>S</kbd> saves.
        </span>
        <span className="grow" />
        <span>
          Leave a cell blank for "not marked yet" — that is not the same as a zero.
        </span>
      </div>
    </Card>
  );
}

/**
 * The live grade preview.
 *
 * Deliberately a rough client-side read of the same bands, purely as feedback
 * while typing. The printed grade always comes from the Rust engine, so a
 * discrepancy here can never reach a report card.
 */
function GradePreview({
  sheet,
  score,
  isAbsent,
  invalid,
}: {
  sheet: MarksSheet;
  score: string;
  isAbsent: boolean;
  invalid: boolean;
}) {
  const systems = useGradingSystems();

  if (isAbsent) return <Badge tone="warning">ABS</Badge>;
  if (invalid) return <Badge tone="danger">!</Badge>;
  if (score.trim() === "") return <span className="subtle">—</span>;

  const system = systems.find((entry) => entry.id === sheet.gradingSystemId);
  if (!system) return <span className="subtle">—</span>;

  const percentage = (Number(score) / sheet.maxScore) * 100;
  if (system.kind === "percentage") {
    return <Badge tone="neutral">{Math.round(percentage)}%</Badge>;
  }

  const band = system.bands.find(
    (entry) => percentage >= entry.lower_bound && percentage <= entry.upper_bound,
  );
  if (!band) return <span className="subtle">—</span>;

  const tone =
    percentage >= 70 ? "success" : percentage >= 50 ? "info" : "danger";
  return <Badge tone={tone}>{band.label}</Badge>;
}

let gradingCache: Awaited<ReturnType<typeof api.listGradingSystems>> | null = null;

function useGradingSystems() {
  const [systems, setSystems] = useState(gradingCache ?? []);

  useEffect(() => {
    if (gradingCache) return;
    api
      .listGradingSystems()
      .then((loaded) => {
        gradingCache = loaded;
        setSystems(loaded);
      })
      .catch(() => undefined);
  }, []);

  return systems;
}

// ---------------------------------------------------------------------------
// Guided form — the third of FR-C3's methods, to the same standard
// ---------------------------------------------------------------------------

function GuidedForm({
  sheet,
  draft,
  index,
  onIndex,
  onScore,
  onToggleAbsent,
}: {
  sheet: MarksSheet;
  draft: Draft;
  index: number;
  onIndex: (index: number) => void;
  onScore: (studentId: string, score: string) => void;
  onToggleAbsent: (studentId: string) => void;
}) {
  const row = sheet.rows[Math.min(index, sheet.rows.length - 1)];
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [index]);

  if (!row) return null;

  const entry = draft[row.studentId] ?? { score: "", isAbsent: false };
  const numeric = Number(entry.score);
  const invalid =
    entry.score.trim() !== "" &&
    (Number.isNaN(numeric) || numeric < 0 || numeric > sheet.maxScore);

  const entered = sheet.rows.filter((candidate) => {
    const value = draft[candidate.studentId];
    return value && (value.score.trim() !== "" || value.isAbsent);
  }).length;

  function go(delta: number) {
    onIndex(Math.min(sheet.rows.length - 1, Math.max(0, index + delta)));
  }

  return (
    <Card
      title={`${sheet.subjectName} — ${sheet.examName}`}
      subtitle={`${sheet.className} · out of ${sheet.maxScore}`}
      actions={
        <Badge tone={entered === sheet.rows.length ? "success" : "neutral"}>
          {entered} of {sheet.rows.length} entered
        </Badge>
      }
    >
      <div style={{ maxWidth: 480, margin: "0 auto" }}>
        <Progress value={index + 1} max={sheet.rows.length} />

        <div style={{ textAlign: "center", margin: "var(--space-6) 0" }}>
          <div className="subtle mono" style={{ fontSize: "var(--text-xs)" }}>
            {row.regNumber}
          </div>
          <div
            style={{
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              marginTop: "var(--space-1)",
            }}
          >
            {row.fullName}
          </div>
          <div className="muted" style={{ fontSize: "var(--text-sm)", marginTop: 2 }}>
            Learner {index + 1} of {sheet.rows.length}
          </div>
        </div>

        <div className="row" style={{ justifyContent: "center", gap: "var(--space-4)" }}>
          <input
            ref={input}
            className={cx("input", invalid && "is-invalid")}
            style={{
              width: 140,
              height: 56,
              fontSize: "var(--text-2xl)",
              textAlign: "center",
              fontWeight: 600,
            }}
            inputMode="decimal"
            value={entry.isAbsent ? "" : entry.score}
            disabled={entry.isAbsent || !sheet.isOpen}
            placeholder={entry.isAbsent ? "ABS" : "—"}
            onChange={(event) => onScore(row.studentId, event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                go(1);
              }
            }}
          />
          <div style={{ minWidth: 80 }}>
            <GradePreview
              sheet={sheet}
              score={entry.score}
              isAbsent={entry.isAbsent}
              invalid={invalid}
            />
          </div>
        </div>

        <div className="row" style={{ justifyContent: "center", marginTop: "var(--space-5)" }}>
          <Button
            size="sm"
            variant={entry.isAbsent ? "primary" : "secondary"}
            icon={entry.isAbsent ? <Check size={14} /> : <UserX size={14} />}
            disabled={!sheet.isOpen}
            onClick={() => onToggleAbsent(row.studentId)}
          >
            {entry.isAbsent ? "Marked absent" : "Mark absent"}
          </Button>
        </div>

        <div className="row-between" style={{ marginTop: "var(--space-8)" }}>
          <Button icon={<ArrowLeft size={15} />} disabled={index === 0} onClick={() => go(-1)}>
            Previous
          </Button>
          <Button
            variant="primary"
            disabled={index >= sheet.rows.length - 1}
            onClick={() => go(1)}
          >
            Next
            <ArrowRight size={15} />
          </Button>
        </div>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// FR-C5 — subject analytics
// ---------------------------------------------------------------------------

function AnalyticsPanel({ analytics }: { analytics: SubjectAnalytics }) {
  const peak = Math.max(1, ...analytics.distribution.map((entry) => entry.count));

  return (
    <div className="stack">
      <div className="grid grid-stats">
        <StatBlock label="Marked" value={String(analytics.entered)} />
        <StatBlock
          label="Average"
          value={
            analytics.meanPercentage === null
              ? "—"
              : `${analytics.meanPercentage.toFixed(1)}%`
          }
        />
        <StatBlock
          label="Pass rate"
          value={analytics.passRate === null ? "—" : `${analytics.passRate}%`}
        />
        <StatBlock
          label="Range"
          value={
            analytics.lowest === null || analytics.highest === null
              ? "—"
              : `${analytics.lowest}–${analytics.highest}`
          }
        />
      </div>

      <div>
        <div className="field-label" style={{ marginBottom: "var(--space-3)" }}>
          Grade distribution
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "flex-end",
            gap: "var(--space-2)",
            height: 150,
          }}
        >
          {analytics.distribution.map((entry) => (
            <div
              key={entry.label}
              style={{ flex: 1, textAlign: "center", display: "flex", flexDirection: "column", justifyContent: "flex-end", height: "100%" }}
            >
              <div
                className="subtle"
                style={{ fontSize: "var(--text-2xs)", marginBottom: 4 }}
              >
                {entry.count || ""}
              </div>
              <div
                style={{
                  height: `${(entry.count / peak) * 100}%`,
                  minHeight: entry.count > 0 ? 4 : 2,
                  background:
                    entry.count > 0 ? "var(--accent)" : "var(--border-subtle)",
                  borderRadius: "var(--radius-sm) var(--radius-sm) 0 0",
                  transition: "height var(--duration-slow) var(--ease-out)",
                }}
              />
              <div
                style={{
                  fontSize: "var(--text-2xs)",
                  marginTop: 6,
                  fontWeight: 600,
                  color: "var(--text-secondary)",
                }}
              >
                {entry.label}
              </div>
            </div>
          ))}
        </div>
      </div>

      {analytics.absent > 0 && (
        <Alert tone="info">
          {analytics.absent} learner{analytics.absent === 1 ? " was" : "s were"}{" "}
          absent and {analytics.absent === 1 ? "is" : "are"} left out of these
          figures — an absence is not counted as a zero.
        </Alert>
      )}
    </div>
  );
}

function StatBlock({ label, value }: { label: string; value: string }) {
  return (
    <div
      style={{
        padding: "var(--space-3) var(--space-4)",
        background: "var(--bg-inset)",
        borderRadius: "var(--radius-md)",
        border: "1px solid var(--border-subtle)",
      }}
    >
      <div className="stat-label">{label}</div>
      <div style={{ fontSize: "var(--text-lg)", fontWeight: 600, marginTop: 4 }}>
        {value}
      </div>
    </div>
  );
}
