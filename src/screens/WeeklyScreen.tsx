/**
 * FR-C12 — weekly assignments. The Subject Teacher records each learner's
 * assignment score for a week of the term. At term end Phantom School Manager summarises the
 * weeks on the back of the report card: each week's score, weeks done and a
 * term mean.
 */

import { useEffect, useMemo, useState } from "react";
import { NotebookPen, Save } from "lucide-react";

import { api } from "../lib/api";
import type { AcademicYearRow, ClassRow, ClassSubjectRow, WeeklySheet } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Button,
  Card,
  EmptyState,
  SelectInput,
  TableSkeleton,
  cx,
} from "../components/ui";

const WEEKS = Array.from({ length: 20 }, (_, index) => index + 1);

type Draft = Record<string, { score: string; remark: string }>;

export function WeeklyScreen() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const features = useStore((state) => state.features);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [classId, setClassId] = useState("");
  const [subjects, setSubjects] = useState<ClassSubjectRow[]>([]);
  const [classSubjectId, setClassSubjectId] = useState("");
  const [termId, setTermId] = useState("");
  const [week, setWeek] = useState(1);
  const [sheet, setSheet] = useState<WeeklySheet | null>(null);
  const [outOf, setOutOf] = useState("10");
  const [draft, setDraft] = useState<Draft>({});
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    Promise.all([api.listClasses(), api.listAcademicYears()])
      .then(([loadedClasses, loadedYears]) => {
        setClasses(loadedClasses);
        setYears(loadedYears);
        if (loadedClasses[0]) setClassId(loadedClasses[0].id);
        const terms = loadedYears.flatMap((year) => year.terms);
        const open = terms.find((term) => term.status === "open") ?? terms[0];
        if (open) setTermId(open.id);
      })
      .catch(reportError);
  }, [reportError]);

  useEffect(() => {
    if (!classId) return;
    api
      .listClassSubjects(classId)
      .then((loaded) => {
        const active = loaded.filter((subject) => subject.status === "active");
        setSubjects(active);
        setClassSubjectId(active[0]?.id ?? "");
      })
      .catch(reportError);
  }, [classId, reportError]);

  const load = () => {
    if (!classSubjectId || !termId) return;
    setLoading(true);
    api
      .loadWeeklySheet(classSubjectId, termId, week)
      .then((loaded) => {
        setSheet(loaded);
        setOutOf(String(loaded.outOf));
        setDraft(
          Object.fromEntries(
            loaded.rows.map((row) => [
              row.studentId,
              { score: row.score === null ? "" : String(row.score), remark: row.remark ?? "" },
            ]),
          ),
        );
      })
      .catch(reportError)
      .finally(() => setLoading(false));
  };

  useEffect(load, [classSubjectId, termId, week]); // eslint-disable-line react-hooks/exhaustive-deps

  const outOfNumber = Number(outOf);
  const problems = useMemo(() => {
    if (!sheet) return 0;
    return sheet.rows.filter((row) => {
      const text = draft[row.studentId]?.score.trim() ?? "";
      if (text === "") return false;
      const value = Number(text);
      return Number.isNaN(value) || value < 0 || value > outOfNumber;
    }).length;
  }, [draft, sheet, outOfNumber]);

  const done = sheet?.rows.filter((row) => (draft[row.studentId]?.score.trim() ?? "") !== "").length ?? 0;

  async function save() {
    if (!sheet) return;
    setSaving(true);
    try {
      const saved = await api.saveWeeklyScores({
        classSubjectId: sheet.classSubjectId,
        termId,
        week,
        outOf: outOfNumber,
        entries: sheet.rows.map((row) => {
          const entry = draft[row.studentId] ?? { score: "", remark: "" };
          return {
            studentId: row.studentId,
            score: entry.score.trim() === "" ? null : Number(entry.score),
            remark: entry.remark.trim() || null,
          };
        }),
      });
      toast("success", `Week ${week} saved for ${saved} learner${saved === 1 ? "" : "s"}.`);
      load();
    } catch (error) {
      reportError(error);
    } finally {
      setSaving(false);
    }
  }

  if (!features.weeklyAssignments) {
    return (
      <div className="page">
        <div className="page-inner">
          <Alert tone="info" title="Weekly assignments is switched off">
            A School Admin can turn it on under Settings, Optional features.
          </Alert>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Weekly assignments</h1>
            <p className="page-description">
              Record each week's assignment score. The term's weeks are
              summarised on the back of every report card.
            </p>
          </div>
        </div>

        <Card>
          <div className="grid-form">
            <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
              {classes.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.name}
                </option>
              ))}
            </SelectInput>
            <SelectInput
              label="Subject"
              value={classSubjectId}
              onChange={(e) => setClassSubjectId(e.target.value)}
            >
              {subjects.map((subject) => (
                <option key={subject.id} value={subject.id}>
                  {subject.displayName}
                </option>
              ))}
            </SelectInput>
            <SelectInput label="Term" value={termId} onChange={(e) => setTermId(e.target.value)}>
              {years.map((year) => (
                <optgroup key={year.id} label={year.label}>
                  {year.terms.map((term) => (
                    <option key={term.id} value={term.id}>
                      {term.name}
                    </option>
                  ))}
                </optgroup>
              ))}
            </SelectInput>
          </div>

          <div className="field" style={{ marginTop: "var(--space-5)" }}>
            <span className="field-label">Week</span>
            <div className="week-picker">
              {WEEKS.map((number) => (
                <button
                  key={number}
                  className={cx(
                    "week-pill",
                    number === week && "is-active",
                    sheet?.weeksRecorded.includes(number) && "is-done",
                  )}
                  onClick={() => setWeek(number)}
                  title={sheet?.weeksRecorded.includes(number) ? "Has scores" : "No scores yet"}
                >
                  {number}
                </button>
              ))}
            </div>
            <span className="field-hint">Weeks with a dot already have scores.</span>
          </div>
        </Card>

        <Card
          title={sheet ? `${sheet.subjectName} • Week ${week}` : "Scores"}
          subtitle={sheet ? `${sheet.className}, ${sheet.termName} • ${done} of ${sheet.rows.length} entered` : undefined}
          actions={
            <label className="row" style={{ gap: "var(--space-2)" }}>
              <span className="muted" style={{ fontSize: "var(--text-sm)" }}>
                Out of
              </span>
              <input
                className="input"
                style={{ width: 80 }}
                inputMode="decimal"
                value={outOf}
                onChange={(event) => setOutOf(event.target.value)}
              />
            </label>
          }
          footer={
            <div className="row-between">
              <span className="field-hint">
                {problems > 0
                  ? `${problems} score${problems === 1 ? " is" : "s are"} outside 0 to ${outOf}.`
                  : "Leave a box empty for a learner who did not hand the assignment in."}
              </span>
              <Button
                variant="primary"
                icon={<Save size={15} />}
                loading={saving}
                disabled={!sheet || problems > 0 || !(outOfNumber >= 1 && outOfNumber <= 100)}
                onClick={() => void save()}
              >
                Save week {week}
              </Button>
            </div>
          }
          flush
        >
          {loading || !sheet ? (
            <TableSkeleton rows={6} />
          ) : sheet.rows.length === 0 ? (
            <EmptyState icon={<NotebookPen size={18} />} title="No learners in this class" />
          ) : (
            <div className="table-wrap" style={{ maxHeight: "56vh" }}>
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th style={{ width: 44 }}>#</th>
                    <th>Learner</th>
                    <th style={{ width: 130 }}>Score</th>
                    <th style={{ width: 70 }}>%</th>
                    <th>Remark</th>
                  </tr>
                </thead>
                <tbody>
                  {sheet.rows.map((row, index) => {
                    const entry = draft[row.studentId] ?? { score: "", remark: "" };
                    const value = Number(entry.score);
                    const bad =
                      entry.score.trim() !== "" &&
                      (Number.isNaN(value) || value < 0 || value > outOfNumber);
                    return (
                      <tr key={row.studentId}>
                        <td className="muted">{index + 1}</td>
                        <td>
                          <div style={{ fontWeight: 500 }}>{row.fullName}</div>
                          <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                            {row.regNumber}
                            {row.streamName ? ` • ${row.streamName}` : ""}
                          </div>
                        </td>
                        <td>
                          <input
                            className={cx("input", bad && "is-invalid")}
                            style={{ height: 34, width: 100 }}
                            inputMode="decimal"
                            data-row={index}
                            value={entry.score}
                            onChange={(event) =>
                              setDraft({
                                ...draft,
                                [row.studentId]: { ...entry, score: event.target.value },
                              })
                            }
                            onKeyDown={(event) => {
                              if (event.key === "Enter" || event.key === "ArrowDown") {
                                event.preventDefault();
                                document
                                  .querySelector<HTMLInputElement>(`input[data-row="${index + 1}"]`)
                                  ?.focus();
                              } else if (event.key === "ArrowUp") {
                                event.preventDefault();
                                document
                                  .querySelector<HTMLInputElement>(`input[data-row="${index - 1}"]`)
                                  ?.focus();
                              }
                            }}
                          />
                        </td>
                        <td className="mono muted">
                          {entry.score.trim() === "" || bad
                            ? "—"
                            : `${Math.round((value / outOfNumber) * 100)}%`}
                        </td>
                        <td>
                          <input
                            className="input"
                            style={{ height: 34 }}
                            placeholder="Optional"
                            value={entry.remark}
                            onChange={(event) =>
                              setDraft({
                                ...draft,
                                [row.studentId]: { ...entry, remark: event.target.value },
                              })
                            }
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
