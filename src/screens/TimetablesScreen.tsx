/**
 * FR-G6 — class timetables, and FR-G7 — the exam timetable.
 *
 * Four tabs:
 *  - Class timetable: a class's (or stream's) week, edited cell by cell or
 *    filled automatically from each subject's lessons per week. A teacher is
 *    never booked in two places.
 *  - Teacher timetable: one teacher's week across every class.
 *  - Exam timetable: each exam's papers by date and time, per class, with
 *    venue and invigilator; generated in one step or built by hand.
 *  - School day: the bell times and which weekdays the school teaches.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  CalendarClock,
  Eraser,
  Plus,
  Printer,
  Save,
  Sparkles,
  Table2,
  Trash2,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  AcademicYearRow,
  ClassRow,
  ClassTimetable,
  DocumentEnvelope,
  ExamPaperRow,
  ExamTimetableDocument,
  PeriodKind,
  PeriodRow,
  SlotRow,
  StreamRow,
  SubjectRow,
  TeacherTimetable,
  TimetableDocument,
  UserSummary,
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
  Segmented,
  SelectInput,
  Switch,
  TextInput,
  cx,
} from "../components/ui";
import {
  ExamTimetableSheet,
  PrintPreview,
  TimetableSheets,
} from "../components/PrintDocument";

type Tab = "class" | "teacher" | "exam" | "day";

const DAY_NAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

const KIND_LABEL: Record<PeriodKind, string> = {
  lesson: "Lesson",
  break: "Break",
  lunch: "Lunch",
  assembly: "Assembly",
  games: "Games",
  prep: "Prep",
};

type Printable =
  | { kind: "timetable"; envelope: DocumentEnvelope<TimetableDocument> }
  | { kind: "exam"; envelope: DocumentEnvelope<ExamTimetableDocument> };

export function TimetablesScreen() {
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);
  const [tab, setTab] = useState<Tab>("class");
  const [printing, setPrinting] = useState<Printable | null>(null);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>
              Back
            </Button>
          }
        >
          {printing.kind === "timetable" ? (
            <TimetableSheets envelope={printing.envelope} />
          ) : (
            <ExamTimetableSheet envelope={printing.envelope} />
          )}
        </PrintPreview>
      </div>
    );
  }

  const tabs: [Tab, string][] = [
    ["class", "Class timetable"],
    ["teacher", "Teacher timetable"],
    ["exam", "Exam timetable"],
    ...(isAdmin ? ([["day", "School day"]] as [Tab, string][]) : []),
  ];

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Timetables</h1>
            <p className="page-description">
              Class and teacher timetables for the week, and the timetable for
              each set of exams. A teacher can never be booked in two places.
            </p>
          </div>
        </div>

        <div className="tabs">
          {tabs.map(([id, label]) => (
            <button key={id} className={cx("tab", tab === id && "is-active")} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </div>

        {tab === "class" && <ClassTab onPrint={setPrinting} />}
        {tab === "teacher" && <TeacherTab onPrint={setPrinting} />}
        {tab === "exam" && <ExamTab onPrint={setPrinting} />}
        {tab === "day" && isAdmin && <SchoolDayTab />}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The grid, shared by class and teacher views
// ---------------------------------------------------------------------------

function WeekGrid({
  periods,
  days,
  cell,
  onCell,
}: {
  periods: PeriodRow[];
  days: number;
  cell: (day: number, period: PeriodRow) => { main: string; sub?: string | null; warn?: string | null } | null;
  onCell?: (day: number, period: PeriodRow) => void;
}) {
  const dayList = Array.from({ length: days }, (_, index) => index + 1);
  return (
    <div className="table-wrap">
      <table className="tt-grid">
        <thead>
          <tr>
            <th className="tt-time">Time</th>
            {dayList.map((day) => (
              <th key={day}>{DAY_NAMES[day - 1]}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {periods.map((period) =>
            period.kind === "lesson" || period.kind === "prep" ? (
              <tr key={period.id}>
                <th className="tt-time">
                  <strong>{period.label}</strong>
                  <span>
                    {period.startTime}–{period.endTime}
                  </span>
                </th>
                {dayList.map((day) => {
                  const content = cell(day, period);
                  return (
                    <td
                      key={day}
                      className={cx(
                        "tt-cell",
                        content && "is-filled",
                        content?.warn && "is-clash",
                        onCell && "is-editable",
                      )}
                      title={content?.warn ?? undefined}
                      onClick={onCell ? () => onCell(day, period) : undefined}
                    >
                      {content ? (
                        <>
                          <strong>{content.main}</strong>
                          {content.sub && <span>{content.sub}</span>}
                          {content.warn && <em>{content.warn}</em>}
                        </>
                      ) : onCell ? (
                        <span className="tt-add">+</span>
                      ) : null}
                    </td>
                  );
                })}
              </tr>
            ) : (
              <tr key={period.id} className={`tt-pause tt-${period.kind}`}>
                <th className="tt-time">
                  <strong>{period.label}</strong>
                  <span>
                    {period.startTime}–{period.endTime}
                  </span>
                </th>
                <td colSpan={days}>{period.label}</td>
              </tr>
            ),
          )}
        </tbody>
      </table>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Class timetable
// ---------------------------------------------------------------------------

function ClassTab({ onPrint }: { onPrint: (printable: Printable) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);
  const features = useStore((state) => state.features);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState("");
  const [streams, setStreams] = useState<StreamRow[]>([]);
  const [streamId, setStreamId] = useState("");
  const [table, setTable] = useState<ClassTimetable | null>(null);
  const [staff, setStaff] = useState<UserSummary[]>([]);
  const [editing, setEditing] = useState<{ day: number; period: PeriodRow } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [unplaced, setUnplaced] = useState<string[]>([]);

  useEffect(() => {
    api
      .listClasses()
      .then((loaded) => {
        setClasses(loaded);
        if (loaded[0]) setClassId(loaded[0].id);
      })
      .catch(reportError);
    if (isAdmin) api.listStaff().then(setStaff).catch(() => undefined);
  }, [reportError, isAdmin]);

  useEffect(() => {
    setStreamId("");
    if (!classId || !features.streams) {
      setStreams([]);
      return;
    }
    api.listStreams(classId).then(setStreams).catch(() => setStreams([]));
  }, [classId, features.streams]);

  const load = useCallback(() => {
    if (!classId) return;
    api.loadClassTimetable(classId, streamId || null).then(setTable).catch(reportError);
  }, [classId, streamId, reportError]);

  useEffect(load, [load]);

  const slotAt = (day: number, periodId: string): SlotRow | undefined =>
    table?.slots.find((slot) => slot.day === day && slot.periodId === periodId);

  const lessonPeriods = table?.periods.filter((p) => p.kind === "lesson").length ?? 0;
  const capacity = lessonPeriods * (table?.days ?? 5);
  const requested = table?.subjects.reduce((sum, s) => sum + s.lessonsPerWeek, 0) ?? 0;

  async function autoFill(scope: "class" | "school", replace: boolean) {
    setBusy("fill");
    try {
      const targets =
        scope === "class"
          ? [{ classId, streamId: streamId || null }]
          : classes.map((entry) => ({ classId: entry.id, streamId: null }));
      const result = await api.autoFillTimetable(targets, replace);
      setUnplaced(result.unplaced);
      toast(
        result.unplaced.length ? "info" : "success",
        `Placed ${result.placed} lesson${result.placed === 1 ? "" : "s"}` +
          (result.unplaced.length ? `; ${result.unplaced.length} subject(s) could not be fully placed.` : "."),
      );
      load();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(null);
    }
  }

  async function print(scope: "class" | "school" | "teachers") {
    try {
      const envelope = await api.buildTimetableDocument({
        scope,
        classId,
        streamId: streamId || null,
      });
      onPrint({ kind: "timetable", envelope });
    } catch (error) {
      reportError(error);
    }
  }

  return (
    <>
      <Card>
        <div className="row" style={{ gap: "var(--space-4)", flexWrap: "wrap", alignItems: "flex-end" }}>
          <div style={{ minWidth: 200 }}>
            <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
              {classes.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.name}
                </option>
              ))}
            </SelectInput>
          </div>
          {streams.length > 0 && (
            <div style={{ minWidth: 160 }}>
              <SelectInput label="Stream" value={streamId} onChange={(e) => setStreamId(e.target.value)}>
                <option value="">Whole class</option>
                {streams.map((stream) => (
                  <option key={stream.id} value={stream.id}>
                    {stream.name}
                  </option>
                ))}
              </SelectInput>
            </div>
          )}
          <span className="grow" />
          <Button icon={<Printer size={15} />} onClick={() => void print("class")}>
            Print
          </Button>
          {isAdmin && (
            <>
              <Button icon={<Printer size={15} />} onClick={() => void print("school")}>
                Print all classes
              </Button>
              <Button icon={<Printer size={15} />} onClick={() => void print("teachers")}>
                Print all teachers
              </Button>
            </>
          )}
        </div>
      </Card>

      {unplaced.length > 0 && (
        <Alert tone="warning" title="Some lessons could not be placed">
          {unplaced.join("; ")}. Add lesson periods, reduce lessons per week, or
          free the teacher elsewhere, then fill again.
        </Alert>
      )}

      <div className="tt-layout">
        <Card
          title={table ? `${table.className}${table.streamName ? ` — ${table.streamName}` : ""}` : "Timetable"}
          subtitle={
            isAdmin
              ? "Click a cell to set the subject, teacher and room. Red means the teacher is booked elsewhere."
              : "This class's week."
          }
          actions={
            isAdmin && table ? (
              <>
                <Button
                  size="sm"
                  icon={<Eraser size={13} />}
                  onClick={() => {
                    if (!window.confirm("Empty this timetable?")) return;
                    api
                      .clearTimetable(classId, streamId || null)
                      .then(() => {
                        toast("success", "Timetable emptied.");
                        load();
                      })
                      .catch(reportError);
                  }}
                >
                  Clear
                </Button>
              </>
            ) : undefined
          }
          flush
        >
          {!table ? (
            <Loading label="Loading" />
          ) : lessonPeriods === 0 ? (
            <EmptyState icon={<Table2 size={18} />} title="The school day has no lessons yet">
              Set the bell times under the School day tab.
            </EmptyState>
          ) : (
            <WeekGrid
              periods={table.periods}
              days={table.days}
              onCell={isAdmin ? (day, period) => setEditing({ day, period }) : undefined}
              cell={(day, period) => {
                const slot = slotAt(day, period.id);
                if (!slot?.subjectName) return null;
                return {
                  main: slot.subjectName,
                  sub: [slot.teacherName, slot.room].filter(Boolean).join(" • ") || null,
                  warn: slot.clash,
                };
              }}
            />
          )}
        </Card>

        {table && (
          <Card
            title="Lessons per week"
            subtitle={`${requested} asked for • ${capacity} lesson slots in the week`}
            footer={
              isAdmin ? (
                <div className="stack" style={{ gap: "var(--space-2)" }}>
                  <Button
                    variant="primary"
                    icon={<Sparkles size={15} />}
                    loading={busy === "fill"}
                    disabled={requested === 0}
                    onClick={() => void autoFill("class", false)}
                  >
                    Fill the empty cells
                  </Button>
                  <Button
                    icon={<Sparkles size={15} />}
                    disabled={requested === 0 || busy === "fill"}
                    onClick={() => {
                      if (window.confirm("Replace this timetable with a fresh one?")) void autoFill("class", true);
                    }}
                  >
                    Start this class again
                  </Button>
                  <Button
                    variant="ghost"
                    icon={<Sparkles size={15} />}
                    disabled={busy === "fill"}
                    onClick={() => {
                      if (
                        window.confirm(
                          "Fill every class's empty cells from its lessons per week? Lessons already placed stay.",
                        )
                      )
                        void autoFill("school", false);
                    }}
                  >
                    Fill every class
                  </Button>
                </div>
              ) : undefined
            }
            flush
          >
            <div className="table-wrap">
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Subject</th>
                    <th style={{ width: 70 }}>Week</th>
                    <th style={{ width: 70 }}>Placed</th>
                  </tr>
                </thead>
                <tbody>
                  {table.subjects.map((subject) => (
                    <tr key={subject.classSubjectId}>
                      <td>
                        <div style={{ fontWeight: 500 }}>{subject.name}</div>
                        <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                          {subject.teacherName ?? "No subject teacher"}
                        </div>
                      </td>
                      <td>
                        {isAdmin ? (
                          <input
                            className="input"
                            style={{ height: 30, width: 52 }}
                            inputMode="numeric"
                            defaultValue={subject.lessonsPerWeek}
                            onBlur={(event) => {
                              const value = Number(event.target.value);
                              if (Number.isNaN(value) || value === subject.lessonsPerWeek) return;
                              api
                                .setLessonsPerWeek(subject.classSubjectId, value)
                                .then(load)
                                .catch(reportError);
                            }}
                          />
                        ) : (
                          subject.lessonsPerWeek
                        )}
                      </td>
                      <td>
                        <Badge
                          tone={
                            subject.lessonsPerWeek === 0
                              ? "neutral"
                              : subject.placed >= subject.lessonsPerWeek
                                ? "success"
                                : "warning"
                          }
                        >
                          {subject.placed}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Card>
        )}
      </div>

      {editing && table && (
        <SlotModal
          table={table}
          staff={staff}
          day={editing.day}
          period={editing.period}
          slot={slotAt(editing.day, editing.period.id)}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
    </>
  );
}

function SlotModal({
  table,
  staff,
  day,
  period,
  slot,
  onClose,
  onSaved,
}: {
  table: ClassTimetable;
  staff: UserSummary[];
  day: number;
  period: PeriodRow;
  slot: SlotRow | undefined;
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [subjectId, setSubjectId] = useState(slot?.subjectId ?? "");
  const [teacherId, setTeacherId] = useState(slot?.teacherId ?? "");
  const [room, setRoom] = useState(slot?.room ?? "");
  const [busy, setBusy] = useState(false);

  function save(empty: boolean) {
    setBusy(true);
    api
      .saveTimetableSlot({
        classId: table.classId,
        streamId: table.streamId,
        day,
        periodId: period.id,
        subjectId: empty ? null : subjectId || null,
        teacherId: empty ? null : teacherId || null,
        room: empty ? null : room.trim() || null,
      })
      .then(onSaved)
      .catch(reportError)
      .finally(() => setBusy(false));
  }

  return (
    <Modal
      open
      title={`${DAY_NAMES[day - 1]}, ${period.label}`}
      description={`${period.startTime}–${period.endTime} • ${table.className}${table.streamName ? ` — ${table.streamName}` : ""}`}
      onClose={onClose}
      footer={
        <>
          {slot && (
            <Button variant="ghost" icon={<Trash2 size={15} />} loading={busy} onClick={() => save(true)}>
              Empty the cell
            </Button>
          )}
          <span className="grow" />
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} disabled={!subjectId} onClick={() => save(false)}>
            Save
          </Button>
        </>
      }
    >
      <div className="stack">
        <SelectInput
          label="Subject"
          value={subjectId}
          onChange={(event) => {
            setSubjectId(event.target.value);
            const load = table.subjects.find((s) => s.subjectId === event.target.value);
            if (load?.teacherId) setTeacherId(load.teacherId);
          }}
        >
          <option value="">Choose a subject</option>
          {table.subjects.map((subject) => (
            <option key={subject.subjectId} value={subject.subjectId}>
              {subject.name} ({subject.placed}/{subject.lessonsPerWeek || "–"})
            </option>
          ))}
        </SelectInput>
        <SelectInput label="Teacher" value={teacherId} onChange={(event) => setTeacherId(event.target.value)}>
          <option value="">No teacher</option>
          {staff.map((person) => (
            <option key={person.id} value={person.id}>
              {person.fullName}
            </option>
          ))}
        </SelectInput>
        <TextInput
          label="Room (optional)"
          value={room}
          placeholder="e.g. Science Lab"
          onChange={(event) => setRoom(event.target.value)}
        />
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Teacher timetable
// ---------------------------------------------------------------------------

function TeacherTab({ onPrint }: { onPrint: (printable: Printable) => void }) {
  const reportError = useStore((state) => state.reportError);
  const session = useStore((state) => state.session);
  const isAdmin = session?.isAdmin ?? false;

  const [staff, setStaff] = useState<UserSummary[]>([]);
  const [userId, setUserId] = useState(session?.userId ?? "");
  const [table, setTable] = useState<TeacherTimetable | null>(null);

  useEffect(() => {
    if (isAdmin) api.listStaff().then(setStaff).catch(reportError);
  }, [isAdmin, reportError]);

  useEffect(() => {
    if (!userId) return;
    setTable(null);
    api.loadTeacherTimetable(userId).then(setTable).catch(reportError);
  }, [userId, reportError]);

  return (
    <>
      <Card>
        <div className="row" style={{ gap: "var(--space-4)", alignItems: "flex-end" }}>
          {isAdmin && (
            <div style={{ minWidth: 240 }}>
              <SelectInput label="Teacher" value={userId} onChange={(e) => setUserId(e.target.value)}>
                {staff.map((person) => (
                  <option key={person.id} value={person.id}>
                    {person.fullName}
                  </option>
                ))}
              </SelectInput>
            </div>
          )}
          <span className="grow" />
          <Button
            icon={<Printer size={15} />}
            disabled={!table || table.slots.length === 0}
            onClick={() =>
              api
                .buildTimetableDocument({ scope: "teacher", userId })
                .then((envelope) => onPrint({ kind: "timetable", envelope }))
                .catch(reportError)
            }
          >
            Print
          </Button>
        </div>
      </Card>

      <Card
        title={table ? table.teacherName : "Timetable"}
        subtitle={table ? `${table.slots.length} lessons a week` : undefined}
        flush
      >
        {!table ? (
          <Loading label="Loading" />
        ) : table.slots.length === 0 ? (
          <EmptyState icon={<Table2 size={18} />} title="No lessons on the timetable yet" />
        ) : (
          <WeekGrid
            periods={table.periods}
            days={table.days}
            cell={(day, period) => {
              const slot = table.slots.find((s) => s.day === day && s.periodId === period.id);
              if (!slot) return null;
              return {
                main: slot.streamName ? `${slot.className} ${slot.streamName}` : slot.className,
                sub: [slot.subjectName, slot.room].filter(Boolean).join(" • ") || null,
              };
            }}
          />
        )}
      </Card>
    </>
  );
}

// ---------------------------------------------------------------------------
// Exam timetable
// ---------------------------------------------------------------------------

function ExamTab({ onPrint }: { onPrint: (printable: Printable) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);

  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [termId, setTermId] = useState("");
  const [examId, setExamId] = useState("");
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [subjects, setSubjects] = useState<SubjectRow[]>([]);
  const [staff, setStaff] = useState<UserSummary[]>([]);
  const [papers, setPapers] = useState<ExamPaperRow[] | null>(null);
  const [printClass, setPrintClass] = useState("");
  const [editing, setEditing] = useState<ExamPaperRow | "new" | null>(null);
  const [generating, setGenerating] = useState(false);

  useEffect(() => {
    Promise.all([api.listAcademicYears(), api.listClasses(), api.listSubjects()])
      .then(([loadedYears, loadedClasses, loadedSubjects]) => {
        setYears(loadedYears);
        setClasses(loadedClasses);
        setSubjects(loadedSubjects.filter((s) => s.status === "active"));
        const terms = loadedYears.flatMap((year) => year.terms);
        const open = terms.find((term) => term.status === "open") ?? terms[0];
        if (open) {
          setTermId(open.id);
          const final = open.exams.find((exam) => exam.isFinal) ?? open.exams[0];
          if (final) setExamId(final.id);
        }
      })
      .catch(reportError);
    if (isAdmin) api.listStaff().then(setStaff).catch(() => undefined);
  }, [reportError, isAdmin]);

  const term = years.flatMap((year) => year.terms).find((entry) => entry.id === termId);

  const load = useCallback(() => {
    if (!examId) {
      setPapers([]);
      return;
    }
    api.listExamPapers(examId).then(setPapers).catch(reportError);
  }, [examId, reportError]);

  useEffect(load, [load]);

  const byDay = useMemo(() => {
    const groups: { date: string; papers: ExamPaperRow[] }[] = [];
    for (const paper of papers ?? []) {
      const last = groups[groups.length - 1];
      if (last && last.date === paper.onDate) last.papers.push(paper);
      else groups.push({ date: paper.onDate, papers: [paper] });
    }
    return groups;
  }, [papers]);

  return (
    <>
      <Card>
        <div className="row" style={{ gap: "var(--space-4)", flexWrap: "wrap", alignItems: "flex-end" }}>
          <div style={{ minWidth: 180 }}>
            <SelectInput
              label="Term"
              value={termId}
              onChange={(event) => {
                setTermId(event.target.value);
                const next = years.flatMap((y) => y.terms).find((t) => t.id === event.target.value);
                setExamId(next?.exams[0]?.id ?? "");
              }}
            >
              {years.map((year) => (
                <optgroup key={year.id} label={year.label}>
                  {year.terms.map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.name}
                    </option>
                  ))}
                </optgroup>
              ))}
            </SelectInput>
          </div>
          <div style={{ minWidth: 220 }}>
            <SelectInput label="Examination" value={examId} onChange={(event) => setExamId(event.target.value)}>
              {(term?.exams ?? []).map((exam) => (
                <option key={exam.id} value={exam.id}>
                  {exam.name}
                </option>
              ))}
            </SelectInput>
          </div>
          <span className="grow" />
          <div style={{ minWidth: 170 }}>
            <SelectInput label="Print for" value={printClass} onChange={(event) => setPrintClass(event.target.value)}>
              <option value="">The whole school</option>
              {classes.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.name}
                </option>
              ))}
            </SelectInput>
          </div>
          <Button
            icon={<Printer size={15} />}
            disabled={!papers || papers.length === 0}
            onClick={() =>
              api
                .buildExamTimetable(examId, printClass || null)
                .then((envelope) => onPrint({ kind: "exam", envelope }))
                .catch(reportError)
            }
          >
            Print
          </Button>
        </div>
      </Card>

      <Card
        title="Papers"
        subtitle={
          papers && papers.length
            ? `${papers.length} papers over ${byDay.length} day${byDay.length === 1 ? "" : "s"} • the first day becomes the exam's date on permits`
            : "Nothing scheduled yet"
        }
        actions={
          isAdmin && examId ? (
            <>
              <Button size="sm" icon={<Sparkles size={13} />} onClick={() => setGenerating(true)}>
                Generate
              </Button>
              <Button size="sm" variant="primary" icon={<Plus size={13} />} onClick={() => setEditing("new")}>
                Add paper
              </Button>
            </>
          ) : undefined
        }
        flush
      >
        {papers === null ? (
          <Loading label="Loading" />
        ) : papers.length === 0 ? (
          <EmptyState icon={<CalendarClock size={18} />} title="No papers yet">
            {isAdmin
              ? "Generate a whole timetable from the subjects each class takes, or add papers one at a time."
              : "The School Admin has not scheduled this examination yet."}
          </EmptyState>
        ) : (
          <div className="exam-days">
            {byDay.map((group) => (
              <section key={group.date} className="exam-day">
                <h3>
                  {new Date(`${group.date}T12:00:00`).toLocaleDateString("en-GB", {
                    weekday: "long",
                    day: "numeric",
                    month: "long",
                  })}
                </h3>
                {group.papers.map((paper) => (
                  <button
                    key={paper.id}
                    className="exam-paper"
                    disabled={!isAdmin}
                    onClick={() => setEditing(paper)}
                  >
                    <span className="exam-time">
                      {paper.startTime}
                      <em>{paper.endTime}</em>
                    </span>
                    <span className="grow">
                      <strong>
                        {paper.subjectName}
                        {paper.paperLabel ? ` — ${paper.paperLabel}` : ""}
                      </strong>
                      <span className="subtle">
                        {paper.classNames.join(", ")}
                        {paper.venue ? ` • ${paper.venue}` : ""}
                        {paper.invigilatorName ? ` • ${paper.invigilatorName}` : ""}
                      </span>
                    </span>
                  </button>
                ))}
              </section>
            ))}
          </div>
        )}
      </Card>

      {editing && (
        <PaperModal
          examId={examId}
          paper={editing === "new" ? null : editing}
          classes={classes}
          subjects={subjects}
          staff={staff}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
      {generating && (
        <GenerateModal
          examId={examId}
          hasPapers={(papers?.length ?? 0) > 0}
          onClose={() => setGenerating(false)}
          onDone={(message) => {
            setGenerating(false);
            toast("success", message);
            load();
          }}
        />
      )}
    </>
  );
}

function PaperModal({
  examId,
  paper,
  classes,
  subjects,
  staff,
  onClose,
  onSaved,
}: {
  examId: string;
  paper: ExamPaperRow | null;
  classes: ClassRow[];
  subjects: SubjectRow[];
  staff: UserSummary[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [subjectId, setSubjectId] = useState(paper?.subjectId ?? subjects[0]?.id ?? "");
  const [label, setLabel] = useState(paper?.paperLabel ?? "");
  const [date, setDate] = useState(paper?.onDate ?? "");
  const [start, setStart] = useState(paper?.startTime ?? "09:00");
  const [end, setEnd] = useState(paper?.endTime ?? "11:00");
  const [venue, setVenue] = useState(paper?.venue ?? "");
  const [invigilatorId, setInvigilatorId] = useState(paper?.invigilatorId ?? "");
  const [classIds, setClassIds] = useState<string[]>(paper?.classIds ?? []);
  const [busy, setBusy] = useState(false);

  return (
    <Modal
      open
      wide
      title={paper ? "Edit paper" : "Add a paper"}
      description="No class can sit two papers at once, and no invigilator can be in two rooms."
      onClose={onClose}
      footer={
        <>
          {paper && (
            <Button
              variant="ghost"
              icon={<Trash2 size={15} />}
              onClick={() => {
                if (!window.confirm("Remove this paper from the timetable?")) return;
                api.deleteExamPaper(paper.id).then(onSaved).catch(reportError);
              }}
            >
              Remove
            </Button>
          )}
          <span className="grow" />
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<Save size={15} />}
            loading={busy}
            disabled={!subjectId || !date || classIds.length === 0}
            onClick={() => {
              setBusy(true);
              api
                .saveExamPaper({
                  id: paper?.id ?? null,
                  examId,
                  subjectId,
                  paperLabel: label.trim() || null,
                  onDate: date,
                  startTime: start,
                  endTime: end,
                  venue: venue.trim() || null,
                  invigilatorId: invigilatorId || null,
                  classIds,
                })
                .then(onSaved)
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="grid-form">
        <SelectInput label="Subject" value={subjectId} onChange={(event) => setSubjectId(event.target.value)}>
          {subjects.map((subject) => (
            <option key={subject.id} value={subject.id}>
              {subject.name}
            </option>
          ))}
        </SelectInput>
        <TextInput label="Paper (optional)" value={label} placeholder="e.g. Paper 1, Oral" onChange={(e) => setLabel(e.target.value)} />
        <div className="field">
          <span className="field-label">Date</span>
          <input type="date" className="input" value={date} onChange={(event) => setDate(event.target.value)} />
        </div>
        <div className="row" style={{ gap: "var(--space-3)" }}>
          <div className="field grow">
            <span className="field-label">Starts</span>
            <input type="time" className="input" value={start} onChange={(event) => setStart(event.target.value)} />
          </div>
          <div className="field grow">
            <span className="field-label">Ends</span>
            <input type="time" className="input" value={end} onChange={(event) => setEnd(event.target.value)} />
          </div>
        </div>
        <TextInput label="Venue (optional)" value={venue} placeholder="e.g. Main hall" onChange={(e) => setVenue(e.target.value)} />
        <SelectInput label="Invigilator (optional)" value={invigilatorId} onChange={(event) => setInvigilatorId(event.target.value)}>
          <option value="">None</option>
          {staff.map((person) => (
            <option key={person.id} value={person.id}>
              {person.fullName}
            </option>
          ))}
        </SelectInput>
      </div>
      <div className="field" style={{ marginTop: "var(--space-4)" }}>
        <span className="field-label">Classes sitting it</span>
        <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
          <button
            className="pick"
            style={{ width: "auto" }}
            onClick={() =>
              setClassIds(classIds.length === classes.length ? [] : classes.map((entry) => entry.id))
            }
          >
            {classIds.length === classes.length ? "None" : "All classes"}
          </button>
          {classes.map((entry) => (
            <button
              key={entry.id}
              className={cx("pick", classIds.includes(entry.id) && "is-selected")}
              style={{ width: "auto" }}
              onClick={() =>
                setClassIds((current) =>
                  current.includes(entry.id) ? current.filter((id) => id !== entry.id) : [...current, entry.id],
                )
              }
            >
              {entry.name}
            </button>
          ))}
        </div>
      </div>
    </Modal>
  );
}

function GenerateModal({
  examId,
  hasPapers,
  onClose,
  onDone,
}: {
  examId: string;
  hasPapers: boolean;
  onClose: () => void;
  onDone: (message: string) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [startDate, setStartDate] = useState("");
  const [sittings, setSittings] = useState([
    { startTime: "09:00", endTime: "11:00" },
    { startTime: "11:30", endTime: "13:30" },
  ]);
  const [skipWeekends, setSkipWeekends] = useState(true);
  const [replace, setReplace] = useState(false);
  const [busy, setBusy] = useState(false);

  return (
    <Modal
      open
      title="Generate the exam timetable"
      description="One paper per subject, sat together by every class that takes it. Core subjects go first."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<Sparkles size={15} />}
            loading={busy}
            disabled={!startDate || sittings.length === 0}
            onClick={() => {
              setBusy(true);
              api
                .autoGenerateExamTimetable({
                  examId,
                  startDate,
                  sessions: sittings,
                  skipWeekends,
                  classIds: [],
                  replace,
                })
                .then((result) =>
                  onDone(
                    result.created
                      ? `Scheduled ${result.created} papers, ${result.firstDate} to ${result.lastDate}.`
                      : "Every subject already has a paper.",
                  ),
                )
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Generate
          </Button>
        </>
      }
    >
      <div className="stack">
        <div className="field">
          <span className="field-label">First day of exams</span>
          <input type="date" className="input" value={startDate} onChange={(event) => setStartDate(event.target.value)} />
        </div>
        <div className="field">
          <span className="field-label">Sittings each day</span>
          <div className="stack" style={{ gap: 6 }}>
            {sittings.map((sitting, index) => (
              <div key={index} className="row" style={{ gap: 6 }}>
                <input
                  type="time"
                  className="input"
                  value={sitting.startTime}
                  onChange={(event) =>
                    setSittings(sittings.map((s, i) => (i === index ? { ...s, startTime: event.target.value } : s)))
                  }
                />
                <span className="muted">to</span>
                <input
                  type="time"
                  className="input"
                  value={sitting.endTime}
                  onChange={(event) =>
                    setSittings(sittings.map((s, i) => (i === index ? { ...s, endTime: event.target.value } : s)))
                  }
                />
                <Button
                  variant="ghost"
                  icon={<Trash2 size={14} />}
                  title="Remove"
                  disabled={sittings.length === 1}
                  onClick={() => setSittings(sittings.filter((_, i) => i !== index))}
                />
              </div>
            ))}
            {sittings.length < 4 && (
              <Button
                size="sm"
                variant="ghost"
                icon={<Plus size={13} />}
                onClick={() => setSittings([...sittings, { startTime: "14:30", endTime: "16:30" }])}
              >
                Add a sitting
              </Button>
            )}
          </div>
        </div>
        <label className="row" style={{ gap: "var(--space-3)" }}>
          <Switch checked={skipWeekends} onChange={setSkipWeekends} label="Skip weekends" />
          <span>Skip Saturdays and Sundays</span>
        </label>
        {hasPapers && (
          <label className="row" style={{ gap: "var(--space-3)" }}>
            <Switch checked={replace} onChange={setReplace} label="Replace existing papers" />
            <span>Replace the papers already scheduled (otherwise only unscheduled subjects are added)</span>
          </label>
        )}
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// The school day
// ---------------------------------------------------------------------------

type DraftPeriod = { id: string | null; label: string; startTime: string; endTime: string; kind: PeriodKind };

function SchoolDayTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [periods, setPeriods] = useState<DraftPeriod[] | null>(null);
  const [days, setDays] = useState<"5" | "6">("5");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getTimetableSetup()
      .then((setup) => {
        setPeriods(setup.periods.map((p) => ({ id: p.id, label: p.label, startTime: p.startTime, endTime: p.endTime, kind: p.kind })));
        setDays(setup.days === 6 ? "6" : "5");
      })
      .catch(reportError);
  }, [reportError]);

  if (!periods) return <Loading />;

  const update = (index: number, patch: Partial<DraftPeriod>) =>
    setPeriods(periods.map((period, i) => (i === index ? { ...period, ...patch } : period)));

  return (
    <Card
      title="School day"
      subtitle="Bell times for every class. Lessons and prep take subjects; breaks, lunch, assembly and games print across the whole row."
      footer={
        <div className="row-between">
          <Segmented
            value={days}
            onChange={setDays}
            options={[
              { value: "5", label: "Monday to Friday" },
              { value: "6", label: "Monday to Saturday" },
            ]}
          />
          <Button
            variant="primary"
            icon={<Save size={15} />}
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .saveTimetableSetup(periods, Number(days))
                .then((setup) => {
                  setPeriods(setup.periods.map((p) => ({ id: p.id, label: p.label, startTime: p.startTime, endTime: p.endTime, kind: p.kind })));
                  toast("success", "School day saved.");
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save
          </Button>
        </div>
      }
      flush
    >
      <div className="table-wrap">
        <table className="table table-compact">
          <thead>
            <tr>
              <th>Name</th>
              <th style={{ width: 130 }}>Starts</th>
              <th style={{ width: 130 }}>Ends</th>
              <th style={{ width: 150 }}>Kind</th>
              <th style={{ width: 50 }} />
            </tr>
          </thead>
          <tbody>
            {periods.map((period, index) => (
              <tr key={period.id ?? `new-${index}`}>
                <td>
                  <input className="input" style={{ height: 32 }} value={period.label} onChange={(e) => update(index, { label: e.target.value })} />
                </td>
                <td>
                  <input type="time" className="input" style={{ height: 32 }} value={period.startTime} onChange={(e) => update(index, { startTime: e.target.value })} />
                </td>
                <td>
                  <input type="time" className="input" style={{ height: 32 }} value={period.endTime} onChange={(e) => update(index, { endTime: e.target.value })} />
                </td>
                <td>
                  <select className="select" style={{ height: 32 }} value={period.kind} onChange={(e) => update(index, { kind: e.target.value as PeriodKind })}>
                    {(Object.keys(KIND_LABEL) as PeriodKind[]).map((kind) => (
                      <option key={kind} value={kind}>
                        {KIND_LABEL[kind]}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <Button size="sm" variant="ghost" icon={<Trash2 size={13} />} title="Remove" onClick={() => setPeriods(periods.filter((_, i) => i !== index))} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div style={{ padding: "var(--space-3) var(--space-4)" }}>
        <Button
          size="sm"
          icon={<Plus size={13} />}
          onClick={() => {
            const last = periods[periods.length - 1];
            setPeriods([
              ...periods,
              {
                id: null,
                label: `Lesson ${periods.filter((p) => p.kind === "lesson").length + 1}`,
                startTime: last?.endTime ?? "08:00",
                endTime: last?.endTime ?? "08:40",
                kind: "lesson",
              },
            ]);
          }}
        >
          Add a period
        </Button>
      </div>
    </Card>
  );
}
