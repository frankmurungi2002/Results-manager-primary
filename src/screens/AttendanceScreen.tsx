/**
 * FR-G9 — the daily attendance register.
 *
 * Three views: mark one class's register for a day (with an optional SMS to
 * the guardian of every absent learner), see the whole school's registers
 * for a day at a glance, and the monthly register grid that prints like the
 * paper register schools keep.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  CalendarCheck,
  CheckCheck,
  ChevronLeft,
  ChevronRight,
  Printer,
  Save,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  AttendanceOverviewRow,
  AttendanceState,
  ClassRow,
  DocumentEnvelope,
  MonthRegister,
  Register,
  StreamRow,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  SelectInput,
  StatTile,
  Switch,
  TableSkeleton,
  cx,
} from "../components/ui";
import { AttendanceRegisterSheet, PrintPreview } from "../components/PrintDocument";

type Tab = "mark" | "school" | "month";

const STATES: { value: AttendanceState; short: string; label: string }[] = [
  { value: "present", short: "P", label: "Present" },
  { value: "absent", short: "A", label: "Absent" },
  { value: "late", short: "L", label: "Late" },
  { value: "excused", short: "E", label: "Excused" },
];

function todayIso(): string {
  const now = new Date();
  const local = new Date(now.getTime() - now.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 10);
}

function shiftDate(iso: string, days: number): string {
  const date = new Date(`${iso}T12:00:00`);
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function prettyDate(iso: string): string {
  return new Date(`${iso}T12:00:00`).toLocaleDateString("en-GB", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

export function AttendanceScreen() {
  const reportError = useStore((state) => state.reportError);
  const features = useStore((state) => state.features);

  const [tab, setTab] = useState<Tab>("mark");
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState("");
  const [streams, setStreams] = useState<StreamRow[]>([]);
  const [streamId, setStreamId] = useState("");
  const [date, setDate] = useState(todayIso());
  const [printing, setPrinting] = useState<DocumentEnvelope<MonthRegister> | null>(null);

  useEffect(() => {
    api
      .listClasses()
      .then((loaded) => {
        setClasses(loaded);
        if (loaded[0]) setClassId(loaded[0].id);
      })
      .catch(reportError);
  }, [reportError]);

  useEffect(() => {
    setStreamId("");
    if (!classId || !features.streams) {
      setStreams([]);
      return;
    }
    api.listStreams(classId).then(setStreams).catch(() => setStreams([]));
  }, [classId, features.streams]);

  const pickers = (
    <>
      <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
        {classes.map((entry) => (
          <option key={entry.id} value={entry.id}>
            {entry.name}
          </option>
        ))}
      </SelectInput>
      {streams.length > 0 && (
        <SelectInput label="Stream" value={streamId} onChange={(e) => setStreamId(e.target.value)}>
          <option value="">Whole class</option>
          {streams.map((stream) => (
            <option key={stream.id} value={stream.id}>
              {stream.name}
            </option>
          ))}
        </SelectInput>
      )}
    </>
  );

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
          <AttendanceRegisterSheet envelope={printing} />
        </PrintPreview>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Attendance</h1>
            <p className="page-description">
              The daily register. It fills "days present" on every report card
              and can text a guardian when their child is absent.
            </p>
          </div>
        </div>

        <div className="tabs">
          {(
            [
              ["mark", "Mark the register"],
              ["school", "Whole school"],
              ["month", "Monthly register"],
            ] as [Tab, string][]
          ).map(([id, label]) => (
            <button key={id} className={cx("tab", tab === id && "is-active")} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </div>

        {tab === "mark" && (
          <MarkRegister
            pickers={pickers}
            classId={classId}
            streamId={streamId || null}
            date={date}
            setDate={setDate}
          />
        )}
        {tab === "school" && (
          <SchoolOverview
            date={date}
            setDate={setDate}
            onOpen={(id) => {
              setClassId(id);
              setTab("mark");
            }}
          />
        )}
        {tab === "month" && (
          <MonthlyRegister
            pickers={pickers}
            classId={classId}
            streamId={streamId || null}
            onPrint={setPrinting}
          />
        )}
      </div>
    </div>
  );
}

function DatePicker({ date, setDate }: { date: string; setDate: (value: string) => void }) {
  return (
    <div className="field">
      <span className="field-label">Date</span>
      <div className="row" style={{ gap: 6 }}>
        <Button icon={<ChevronLeft size={15} />} title="Day before" onClick={() => setDate(shiftDate(date, -1))} />
        <input
          type="date"
          className="input"
          style={{ width: 170 }}
          value={date}
          max={todayIso()}
          onChange={(event) => event.target.value && setDate(event.target.value)}
        />
        <Button
          icon={<ChevronRight size={15} />}
          title="Next day"
          disabled={date >= todayIso()}
          onClick={() => setDate(shiftDate(date, 1))}
        />
        {date !== todayIso() && (
          <Button variant="ghost" onClick={() => setDate(todayIso())}>
            Today
          </Button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Marking
// ---------------------------------------------------------------------------

type Draft = Record<string, { state: AttendanceState | null; note: string }>;

function MarkRegister({
  pickers,
  classId,
  streamId,
  date,
  setDate,
}: {
  pickers: React.ReactNode;
  classId: string;
  streamId: string | null;
  date: string;
  setDate: (value: string) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [register, setRegister] = useState<Register | null>(null);
  const [draft, setDraft] = useState<Draft>({});
  const [notify, setNotify] = useState(true);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const load = useCallback(() => {
    if (!classId) return;
    setLoading(true);
    api
      .loadRegister(classId, streamId, date)
      .then((loaded) => {
        setRegister(loaded);
        setDraft(
          Object.fromEntries(
            loaded.rows.map((row) => [row.studentId, { state: row.state, note: row.note ?? "" }]),
          ),
        );
      })
      .catch(reportError)
      .finally(() => setLoading(false));
  }, [classId, streamId, date, reportError]);

  useEffect(load, [load]);

  const counts = useMemo(() => {
    const tally = { present: 0, absent: 0, late: 0, excused: 0, unmarked: 0 };
    for (const row of register?.rows ?? []) {
      const state = draft[row.studentId]?.state;
      if (state) tally[state] += 1;
      else tally.unmarked += 1;
    }
    return tally;
  }, [draft, register]);

  function setState(studentId: string, state: AttendanceState) {
    setDraft((current) => ({
      ...current,
      [studentId]: { note: current[studentId]?.note ?? "", state },
    }));
  }

  async function save() {
    if (!register) return;
    setSaving(true);
    try {
      const result = await api.saveRegister({
        classId,
        streamId,
        onDate: date,
        notifyAbsent: notify,
        entries: register.rows
          .filter((row) => draft[row.studentId]?.state)
          .map((row) => ({
            studentId: row.studentId,
            state: draft[row.studentId]!.state!,
            note: draft[row.studentId]!.note.trim() || null,
          })),
      });
      toast(
        "success",
        `Register saved: ${result.present + result.late} in school, ${result.absent} absent` +
          (result.textsQueued ? `. ${result.textsQueued} guardian${result.textsQueued === 1 ? "" : "s"} being texted.` : "."),
      );
      load();
    } catch (error) {
      reportError(error);
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <Card>
        <div className="grid-form">
          {pickers}
          <DatePicker date={date} setDate={setDate} />
        </div>
      </Card>

      {register?.isWeekend && (
        <Alert tone="warning" title={`${register.weekday} is a weekend day`}>
          Only mark it if the school was open.
        </Alert>
      )}
      {register && !register.termName && (
        <Alert tone="warning" title="No term covers this date">
          Set the term's start and end dates under Terms &amp; exams before marking.
        </Alert>
      )}

      <Card
        title={register ? `${register.className} • ${prettyDate(date)}` : "Register"}
        subtitle={
          register
            ? register.alreadyMarked
              ? `Marked${register.markedBy ? ` by ${register.markedBy}` : ""}. Changes replace the saved marks.`
              : `Not marked yet${register.termName ? ` • ${register.termName}` : ""}`
            : undefined
        }
        actions={
          <Button
            icon={<CheckCheck size={15} />}
            disabled={!register || register.rows.length === 0}
            onClick={() =>
              register &&
              setDraft((current) =>
                Object.fromEntries(
                  register.rows.map((row) => [
                    row.studentId,
                    {
                      note: current[row.studentId]?.note ?? "",
                      state: current[row.studentId]?.state ?? "present",
                    },
                  ]),
                ),
              )
            }
          >
            Everyone else present
          </Button>
        }
        footer={
          <div className="row-between" style={{ flexWrap: "wrap", gap: "var(--space-3)" }}>
            <label className="row" style={{ gap: "var(--space-3)" }}>
              <Switch checked={notify} onChange={setNotify} label="Text guardians of absent learners" />
              <span style={{ fontWeight: 500 }}>Text the guardian of every learner newly marked absent</span>
            </label>
            <Button
              variant="primary"
              icon={<Save size={15} />}
              loading={saving}
              disabled={!register || counts.unmarked === register.rows.length || !register.termName}
              onClick={() => void save()}
            >
              Save register
            </Button>
          </div>
        }
        flush
      >
        {loading || !register ? (
          <TableSkeleton rows={8} />
        ) : register.rows.length === 0 ? (
          <EmptyState icon={<CalendarCheck size={18} />} title="No learners in this class" />
        ) : (
          <>
            <div className="att-counts">
              <span className="att-chip att-present">{counts.present} present</span>
              <span className="att-chip att-absent">{counts.absent} absent</span>
              <span className="att-chip att-late">{counts.late} late</span>
              <span className="att-chip att-excused">{counts.excused} excused</span>
              {counts.unmarked > 0 && <span className="att-chip">{counts.unmarked} not marked</span>}
            </div>
            <div className="table-wrap" style={{ maxHeight: "56vh" }}>
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th style={{ width: 44 }}>#</th>
                    <th>Learner</th>
                    <th style={{ width: 270 }}>Mark</th>
                    <th>Note</th>
                  </tr>
                </thead>
                <tbody>
                  {register.rows.map((row, index) => {
                    const entry = draft[row.studentId];
                    return (
                      <tr key={row.studentId}>
                        <td className="muted">{index + 1}</td>
                        <td>
                          <div style={{ fontWeight: 500 }}>
                            {row.fullName}
                            {row.absencesThisTerm >= 3 && (
                              <span style={{ marginLeft: 8 }}>
                                <Badge tone="warning">{row.absencesThisTerm} absences this term</Badge>
                              </span>
                            )}
                          </div>
                          <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                            {row.regNumber}
                            {row.streamName ? ` • ${row.streamName}` : ""}
                            {!row.guardianPhone ? " • no guardian phone" : ""}
                          </div>
                        </td>
                        <td>
                          <div className="att-marks" role="radiogroup" aria-label={row.fullName}>
                            {STATES.map((option) => (
                              <button
                                key={option.value}
                                role="radio"
                                aria-checked={entry?.state === option.value}
                                title={option.label}
                                className={cx(
                                  "att-mark",
                                  `att-${option.value}`,
                                  entry?.state === option.value && "is-on",
                                )}
                                onClick={() => setState(row.studentId, option.value)}
                              >
                                {option.short}
                                <span>{option.label}</span>
                              </button>
                            ))}
                          </div>
                        </td>
                        <td>
                          <input
                            className="input"
                            style={{ height: 32 }}
                            placeholder={entry?.state === "absent" || entry?.state === "excused" ? "Reason (optional)" : ""}
                            value={entry?.note ?? ""}
                            onChange={(event) =>
                              setDraft((current) => ({
                                ...current,
                                [row.studentId]: {
                                  state: current[row.studentId]?.state ?? null,
                                  note: event.target.value,
                                },
                              }))
                            }
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </>
        )}
      </Card>
    </>
  );
}

// ---------------------------------------------------------------------------
// Whole school
// ---------------------------------------------------------------------------

function SchoolOverview({
  date,
  setDate,
  onOpen,
}: {
  date: string;
  setDate: (value: string) => void;
  onOpen: (classId: string) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [rows, setRows] = useState<AttendanceOverviewRow[] | null>(null);

  useEffect(() => {
    setRows(null);
    api.attendanceOverview(date).then(setRows).catch(reportError);
  }, [date, reportError]);

  const learners = rows?.reduce((sum, row) => sum + row.learners, 0) ?? 0;
  const marked = rows?.filter((row) => row.marked > 0).length ?? 0;
  const inSchool = rows?.reduce((sum, row) => sum + row.present + row.late, 0) ?? 0;
  const absent = rows?.reduce((sum, row) => sum + row.absent, 0) ?? 0;
  const markedLearners = rows?.reduce((sum, row) => sum + row.marked, 0) ?? 0;

  return (
    <>
      <Card>
        <DatePicker date={date} setDate={setDate} />
      </Card>

      <div className="grid grid-stats">
        <StatTile label="Registers marked" value={`${marked} of ${rows?.length ?? 0}`} />
        <StatTile label="In school" value={inSchool} meta={`of ${learners} learners`} />
        <StatTile label="Absent" value={absent} />
        <StatTile
          label="Attendance rate"
          value={markedLearners ? `${Math.round((inSchool / markedLearners) * 100)}%` : "—"}
          meta="of learners marked"
        />
      </div>

      <Card title={prettyDate(date)} subtitle="Every class's register for the day" flush>
        {rows === null ? (
          <TableSkeleton rows={6} />
        ) : (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Class</th>
                  <th>Register</th>
                  <th style={{ width: 90 }}>Present</th>
                  <th style={{ width: 90 }}>Absent</th>
                  <th style={{ width: 80 }}>Late</th>
                  <th style={{ width: 200 }}>Rate</th>
                  <th style={{ width: 90 }} />
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => {
                  const rate = row.marked ? Math.round(((row.present + row.late) / row.marked) * 100) : null;
                  return (
                    <tr key={row.classId}>
                      <td>
                        <div style={{ fontWeight: 600 }}>{row.className}</div>
                        <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                          {row.learners} learners
                          {row.classTeacherName ? ` • ${row.classTeacherName}` : ""}
                        </div>
                      </td>
                      <td>
                        {row.marked === 0 ? (
                          <Badge tone="warning">Not marked</Badge>
                        ) : (
                          <span>
                            <Badge tone="success">Marked</Badge>
                            {row.markedBy && (
                              <span className="subtle" style={{ fontSize: "var(--text-2xs)", marginLeft: 6 }}>
                                by {row.markedBy}
                              </span>
                            )}
                          </span>
                        )}
                      </td>
                      <td>{row.marked ? row.present : "—"}</td>
                      <td>{row.marked ? row.absent : "—"}</td>
                      <td>{row.marked ? row.late : "—"}</td>
                      <td>
                        {rate === null ? (
                          <span className="subtle">—</span>
                        ) : (
                          <div className="row" style={{ gap: 8 }}>
                            <div className="progress" style={{ flex: 1 }}>
                              <div className="progress-bar" style={{ width: `${rate}%` }} />
                            </div>
                            <span className="mono">{rate}%</span>
                          </div>
                        )}
                      </td>
                      <td>
                        <Button size="sm" variant="ghost" onClick={() => onOpen(row.classId)}>
                          Open
                        </Button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </>
  );
}

// ---------------------------------------------------------------------------
// Monthly register
// ---------------------------------------------------------------------------

function MonthlyRegister({
  pickers,
  classId,
  streamId,
  onPrint,
}: {
  pickers: React.ReactNode;
  classId: string;
  streamId: string | null;
  onPrint: (envelope: DocumentEnvelope<MonthRegister>) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [month, setMonth] = useState(todayIso().slice(0, 7));
  const [data, setData] = useState<MonthRegister | null>(null);

  const [year, monthNumber] = month.split("-").map(Number) as [number, number];

  useEffect(() => {
    if (!classId || !year || !monthNumber) return;
    setData(null);
    api.attendanceMonth(classId, streamId, year, monthNumber).then(setData).catch(reportError);
  }, [classId, streamId, year, monthNumber, reportError]);

  return (
    <>
      <Card>
        <div className="grid-form">
          {pickers}
          <div className="field">
            <span className="field-label">Month</span>
            <input
              type="month"
              className="input"
              value={month}
              onChange={(event) => event.target.value && setMonth(event.target.value)}
            />
          </div>
        </div>
      </Card>

      <Card
        title={data ? `${data.className}${data.streamName ? ` — ${data.streamName}` : ""} • ${data.monthLabel}` : "Monthly register"}
        subtitle={
          data
            ? `${data.totals.daysMarked} days marked • attendance ${data.totals.rate === null ? "—" : `${data.totals.rate}%`}`
            : undefined
        }
        actions={
          <Button
            variant="primary"
            icon={<Printer size={15} />}
            disabled={!data || data.rows.length === 0}
            onClick={() =>
              api
                .buildAttendanceRegister(classId, streamId, year, monthNumber)
                .then(onPrint)
                .catch(reportError)
            }
          >
            Print register
          </Button>
        }
        flush
      >
        {!data ? (
          <TableSkeleton rows={8} />
        ) : data.rows.length === 0 ? (
          <EmptyState icon={<CalendarCheck size={18} />} title="No learners in this class" />
        ) : (
          <div className="table-wrap" style={{ maxHeight: "60vh" }}>
            <table className="table table-compact att-month">
              <thead>
                <tr>
                  <th className="att-name">Learner</th>
                  {data.days.map((day) => (
                    <th key={day.date} className="att-day" title={day.date}>
                      <span>{day.initial}</span>
                      {day.day}
                    </th>
                  ))}
                  <th className="att-day">P</th>
                  <th className="att-day">A</th>
                  <th className="att-day">L</th>
                  <th className="att-day">%</th>
                </tr>
              </thead>
              <tbody>
                {data.rows.map((row) => (
                  <tr key={row.studentId}>
                    <td className="att-name">{row.fullName}</td>
                    {row.marks.map((mark, index) => (
                      <td key={index} className={cx("att-cell", mark && `att-cell-${mark}`)}>
                        {mark}
                      </td>
                    ))}
                    <td className="att-cell">{row.present}</td>
                    <td className="att-cell">{row.absent}</td>
                    <td className="att-cell">{row.late}</td>
                    <td className="att-cell mono">{row.rate === null ? "—" : Math.round(row.rate)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </>
  );
}
