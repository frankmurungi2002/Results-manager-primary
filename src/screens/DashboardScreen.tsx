/** The landing screen: where the school stands, and what needs doing next. */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  CalendarDays,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  ClipboardCheck,
  ClipboardList,
  DatabaseBackup,
  FileText,
  GraduationCap,
  LayoutGrid,
  Table2,
  TriangleAlert,
  UserPlus,
  Users,
} from "lucide-react";

import { api } from "../lib/api";
import type { AcademicYearRow, ClassRow, DashboardSummary } from "../lib/types";
import { useStore, type ScreenId } from "../state/store";
import { Button, EmptyState, formatDate, initials, relativeTime } from "../components/ui";

export function DashboardScreen() {
  const session = useStore((state) => state.session);
  const navigate = useStore((state) => state.navigate);
  const reportError = useStore((state) => state.reportError);

  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([api.dashboardSummary(), api.listClasses(), api.listAcademicYears()])
      .then(([loadedSummary, loadedClasses, loadedYears]) => {
        setSummary(loadedSummary);
        setClasses(loadedClasses);
        setYears(loadedYears);
      })
      .catch(reportError)
      .finally(() => setLoading(false));
  }, [reportError]);

  if (loading) return <DashboardSkeleton />;
  if (!summary || !session) return null;

  const isAdmin = session.isAdmin;
  const marksPercent =
    summary.marksExpectedThisTerm > 0
      ? Math.round((summary.marksEnteredThisTerm / summary.marksExpectedThisTerm) * 100)
      : 0;
  const openTerm = years.flatMap((year) => year.terms).find((term) => term.status === "open") ?? null;
  const attention = attentionItems(summary, isAdmin, marksPercent);

  return (
    <div className="page">
      <div className="page-inner dash">
        {/* ---- Greeting and the two things people come here to do ---- */}
        <header className="dash-head">
          <div>
            <p className="dash-eyebrow">
              {new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
            </p>
            <h1 className="dash-title">
              Good {greeting()}, {firstName(session.fullName)}
            </h1>
            <p className="dash-sub">
              {summary.currentTerm
                ? `${summary.currentTerm} of ${summary.academicYear} is open for marks entry.`
                : "No term is open yet. Open one from Terms & exams to start entering marks."}
            </p>
          </div>
          <div className="dash-actions">
            <Button icon={<ClipboardList size={15} />} onClick={() => navigate("marks")}>
              Enter marks
            </Button>
            <Button variant="primary" icon={<FileText size={15} />} onClick={() => navigate("reports")}>
              Print reports
            </Button>
          </div>
        </header>

        {/* ---- The four numbers ---- */}
        <section className="dash-stats">
          <StatCard
            tone="lavender"
            icon={<GraduationCap size={20} />}
            label="Learners"
            value={summary.learners.toLocaleString()}
            meta={`${summary.boys} boys · ${summary.girls} girls`}
          />
          <StatCard
            tone="sky"
            icon={<LayoutGrid size={20} />}
            label="Classes"
            value={summary.classes.toLocaleString()}
            meta={`${summary.subjects} subjects in the catalogue`}
          />
          <StatCard
            tone="peach"
            icon={<Users size={20} />}
            label="Teachers"
            value={summary.teachers.toLocaleString()}
            meta={summary.teachers === 0 ? "None added yet" : "Active accounts"}
          />
          <StatCard
            tone="mint"
            icon={<ClipboardCheck size={20} />}
            label="Marks this term"
            value={`${marksPercent}%`}
            meta={`${summary.marksEnteredThisTerm.toLocaleString()} of ${summary.marksExpectedThisTerm.toLocaleString()} entered`}
            meter={marksPercent}
          />
        </section>

        {/* ---- Charts, activity, calendar, attention ---- */}
        <section className="dash-grid">
          <div className="dash-col">
            <LearnersByClass classes={classes} />
            <NeedsAttention items={attention} onGo={(screen, params) => navigate(screen, params)} />
            {isAdmin && (
              <RecentActivity entries={summary.recentActivity} onViewAll={() => navigate("audit")} />
            )}
          </div>

          <div className="dash-col">
            {isAdmin && (
              <div className="dash-quick">
                <Button icon={<UserPlus size={15} />} onClick={() => navigate("learners")}>
                  Add learner
                </Button>
                <Button icon={<Users size={15} />} onClick={() => navigate("staff")}>
                  Add teacher
                </Button>
                <Button
                  icon={<DatabaseBackup size={15} />}
                  onClick={() => navigate("settings", { tab: "backup" })}
                  className="dash-quick-wide"
                >
                  Back up now
                </Button>
              </div>
            )}
            <BoysAndGirls boys={summary.boys} girls={summary.girls} total={summary.learners} />
            <TermCalendar term={openTerm} />
          </div>
        </section>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stat cards
// ---------------------------------------------------------------------------

function StatCard({
  tone,
  icon,
  label,
  value,
  meta,
  meter,
}: {
  tone: "lavender" | "sky" | "peach" | "mint";
  icon: ReactNode;
  label: string;
  value: string;
  meta: string;
  meter?: number;
}) {
  return (
    <div className={`dash-stat dash-stat-${tone}`}>
      <div className="dash-stat-top">
        <span className="dash-stat-label">{label}</span>
        <span className="dash-stat-icon">{icon}</span>
      </div>
      <div className="dash-stat-value">{value}</div>
      {meter !== undefined && (
        <div className="dash-meter" role="progressbar" aria-valuenow={meter} aria-valuemin={0} aria-valuemax={100}>
          <span style={{ width: `${Math.min(100, Math.max(0, meter))}%` }} />
        </div>
      )}
      <div className="dash-stat-meta">{meta}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Learners by class — one series, so no legend: the title names it.
// ---------------------------------------------------------------------------

function LearnersByClass({ classes }: { classes: ClassRow[] }) {
  const [asTable, setAsTable] = useState(false);
  const active = classes.filter((entry) => entry.status === "active");
  const step = niceStep(Math.max(0, ...active.map((entry) => entry.learnerCount)));
  const max = step * 4;
  const ticks = [0, 1, 2, 3, 4].map((index) => index * step);

  return (
    <div className="dash-card">
      <div className="dash-card-head">
        <div>
          <h2 className="dash-card-title">Learners by class</h2>
          <p className="dash-card-sub">Active learners enrolled this academic year</p>
        </div>
        <button
          type="button"
          className="dash-toggle"
          onClick={() => setAsTable((value) => !value)}
          aria-pressed={asTable}
        >
          <Table2 size={14} />
          {asTable ? "Chart" : "Table"}
        </button>
      </div>

      {active.length === 0 ? (
        <EmptyState icon={<LayoutGrid size={18} />} title="No classes yet">
          Classes appear here once they are set up.
        </EmptyState>
      ) : asTable ? (
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th>Class</th>
                <th className="num">Learners</th>
                <th>Class teacher</th>
              </tr>
            </thead>
            <tbody>
              {active.map((entry) => (
                <tr key={entry.id}>
                  <td>{entry.name}</td>
                  <td className="num">{entry.learnerCount}</td>
                  <td className="muted">{entry.classTeacherName ?? "Not assigned"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="colchart">
          <div className="colchart-axis" aria-hidden="true">
            {[...ticks].reverse().map((tick) => (
              <span key={tick}>{tick.toLocaleString()}</span>
            ))}
          </div>
          <div className="colchart-plot">
            <div className="colchart-grid" aria-hidden="true">
              {ticks.map((tick) => (
                <span key={tick} />
              ))}
            </div>
            <div className="colchart-cols" role="list" aria-label="Learners by class">
              {active.map((entry) => {
                const height = max > 0 ? (entry.learnerCount / max) * 100 : 0;
                return (
                  <div
                    key={entry.id}
                    className="colchart-col"
                    role="listitem"
                    tabIndex={0}
                    aria-label={`${entry.name}: ${entry.learnerCount} learners`}
                  >
                    <div className="colchart-bar-area">
                      <div className="colchart-bar" style={{ height: `${height}%` }}>
                        <span className="colchart-value">{entry.learnerCount}</span>
                        <span className="dash-tip" role="tooltip">
                          <strong>{entry.name}</strong>
                          <span>
                            {entry.learnerCount} learner{entry.learnerCount === 1 ? "" : "s"}
                          </span>
                          <span className="dash-tip-muted">
                            {entry.classTeacherName
                              ? `Class teacher: ${entry.classTeacherName}`
                              : "No class teacher yet"}
                          </span>
                        </span>
                      </div>
                    </div>
                    <span className="colchart-label">{entry.code}</span>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** A round tick step so four steps reach at or above the largest value. */
function niceStep(value: number): number {
  const steps = [1, 2, 3, 4, 5, 10, 15, 20, 25, 50, 75, 100, 150, 200, 250, 500, 1000];
  return steps.find((step) => step * 4 >= value) ?? Math.ceil(value / 4000) * 1000;
}

// ---------------------------------------------------------------------------
// Boys and girls — two series, so a legend with counts carries identity.
// ---------------------------------------------------------------------------

function BoysAndGirls({ boys, girls, total }: { boys: number; girls: number; total: number }) {
  const unrecorded = Math.max(0, total - boys - girls);
  const segments = [
    { key: "boys", label: "Boys", value: boys, className: "donut-boys" },
    { key: "girls", label: "Girls", value: girls, className: "donut-girls" },
    { key: "none", label: "Not recorded", value: unrecorded, className: "donut-none" },
  ].filter((segment) => segment.value > 0);

  const radius = 52;
  const circumference = 2 * Math.PI * radius;
  const gap = segments.length > 1 ? 3 : 0;
  let offset = 0;

  return (
    <div className="dash-card">
      <div className="dash-card-head">
        <div>
          <h2 className="dash-card-title">Boys and girls</h2>
          <p className="dash-card-sub">Across all active learners</p>
        </div>
      </div>

      {total === 0 ? (
        <EmptyState icon={<GraduationCap size={18} />} title="No learners yet" />
      ) : (
        <div className="donut-wrap">
          <svg className="donut" viewBox="0 0 140 140" role="img" aria-label={`${boys} boys and ${girls} girls`}>
            <circle cx="70" cy="70" r={radius} className="donut-track" />
            {segments.map((segment) => {
              const length = (segment.value / total) * circumference;
              const drawn = Math.max(0, length - gap);
              const element = (
                <circle
                  key={segment.key}
                  cx="70"
                  cy="70"
                  r={radius}
                  className={`donut-seg ${segment.className}`}
                  strokeDasharray={`${drawn} ${circumference - drawn}`}
                  strokeDashoffset={-offset}
                  transform="rotate(-90 70 70)"
                >
                  <title>
                    {segment.label}: {segment.value} ({Math.round((segment.value / total) * 100)}%)
                  </title>
                </circle>
              );
              offset += length;
              return element;
            })}
            <text x="70" y="66" className="donut-total">
              {total.toLocaleString()}
            </text>
            <text x="70" y="84" className="donut-caption">
              learners
            </text>
          </svg>

          <ul className="donut-legend">
            {segments.map((segment) => (
              <li key={segment.key}>
                <span className={`donut-swatch ${segment.className}`} />
                <span className="grow">{segment.label}</span>
                <span className="donut-count">{segment.value.toLocaleString()}</span>
                <span className="donut-pct">{Math.round((segment.value / total) * 100)}%</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Calendar — this month, with the open term's dates and its exams marked
// ---------------------------------------------------------------------------

type TermLike = AcademicYearRow["terms"][number];

function TermCalendar({ term }: { term: TermLike | null }) {
  const today = new Date();
  const [month, setMonth] = useState(() => new Date(today.getFullYear(), today.getMonth(), 1));

  const events = useMemo(() => {
    const list: { date: string; label: string; kind: "term" | "exam" }[] = [];
    if (!term) return list;
    if (term.startDate) list.push({ date: term.startDate, label: `${term.name} begins`, kind: "term" });
    if (term.endDate) list.push({ date: term.endDate, label: `${term.name} ends`, kind: "term" });
    for (const exam of term.exams) {
      if (exam.scheduledDate) list.push({ date: exam.scheduledDate, label: exam.name, kind: "exam" });
    }
    return list.sort((a, b) => a.date.localeCompare(b.date));
  }, [term]);

  const eventDays = new Map(events.map((event) => [event.date.slice(0, 10), event.kind]));
  const upcoming = events.filter((event) => event.date.slice(0, 10) >= isoDay(today)).slice(0, 3);

  const first = month.getDay(); // Sunday = 0
  const daysInMonth = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
  const cells: (number | null)[] = [
    ...Array.from({ length: first }, () => null),
    ...Array.from({ length: daysInMonth }, (_, index) => index + 1),
  ];

  return (
    <div className="dash-card">
      <div className="dash-card-head">
        <h2 className="dash-card-title">
          {month.toLocaleDateString(undefined, { month: "long", year: "numeric" })}
        </h2>
        <div className="cal-nav">
          <button
            type="button"
            aria-label="Previous month"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            aria-label="Next month"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}
          >
            <ChevronRight size={16} />
          </button>
        </div>
      </div>

      <div className="cal-grid" role="grid">
        {["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].map((day) => (
          <span key={day} className="cal-dow">
            {day}
          </span>
        ))}
        {cells.map((day, index) => {
          if (day === null) return <span key={`blank-${index}`} />;
          const iso = isoDay(new Date(month.getFullYear(), month.getMonth(), day));
          const isToday = iso === isoDay(today);
          const kind = eventDays.get(iso);
          return (
            <span key={iso} className={`cal-day${isToday ? " is-today" : ""}`}>
              {day}
              {kind && <span className={`cal-dot cal-dot-${kind}`} />}
            </span>
          );
        })}
      </div>

      <div className="cal-upcoming">
        <h3 className="dash-mini-title">Coming up</h3>
        {upcoming.length === 0 ? (
          <p className="dash-card-sub">
            {term ? "No dates set for this term. Add them in Terms & exams." : "No term is open."}
          </p>
        ) : (
          upcoming.map((event) => (
            <div key={`${event.date}-${event.label}`} className="cal-event">
              <span className={`cal-event-icon cal-event-${event.kind}`}>
                {event.kind === "exam" ? <ClipboardList size={15} /> : <CalendarDays size={15} />}
              </span>
              <span className="grow">
                <span className="cal-event-title">{event.label}</span>
                <span className="cal-event-meta">{formatDate(event.date)}</span>
              </span>
              <span className="cal-event-when">{daysUntil(event.date)}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function isoDay(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function daysUntil(iso: string): string {
  const target = new Date(`${iso.slice(0, 10)}T00:00:00`);
  const start = new Date();
  start.setHours(0, 0, 0, 0);
  const days = Math.round((target.getTime() - start.getTime()) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Tomorrow";
  return `In ${days} days`;
}

// ---------------------------------------------------------------------------
// Needs attention
// ---------------------------------------------------------------------------

interface AttentionItem {
  key: string;
  tone: "warning" | "danger" | "info";
  tag: string;
  title: string;
  detail: string;
  action: string;
  screen: ScreenId;
  params?: Record<string, string>;
}

function attentionItems(summary: DashboardSummary, isAdmin: boolean, marksPercent: number): AttentionItem[] {
  const items: AttentionItem[] = [];
  const backupAgeDays = summary.lastBackupAt
    ? (Date.now() - new Date(summary.lastBackupAt).getTime()) / 86_400_000
    : Infinity;

  if (isAdmin && backupAgeDays > 3) {
    items.push({
      key: "backup",
      tone: "danger",
      tag: "Backup",
      title: summary.lastBackupAt ? "Backup is out of date" : "Never backed up",
      detail: summary.lastBackupAt
        ? `The last backup was ${relativeTime(summary.lastBackupAt)}. A term's marks are hard to rebuild.`
        : "Take a backup, and keep one drive off site.",
      action: "Back up now",
      screen: "settings",
      params: { tab: "backup" },
    });
  }
  if (!summary.currentTerm) {
    items.push({
      key: "term",
      tone: "warning",
      tag: "Calendar",
      title: "No term is open",
      detail: "Open a term so teachers know where to enter marks.",
      action: "Open a term",
      screen: "calendar",
    });
  }
  if (summary.learners === 0) {
    items.push({
      key: "learners",
      tone: "warning",
      tag: "Learners",
      title: "No learners yet",
      detail: "Add learners one at a time, or import the school's spreadsheet.",
      action: "Add learners",
      screen: "learners",
    });
  }
  if (isAdmin && summary.teachers === 0) {
    items.push({
      key: "teachers",
      tone: "info",
      tag: "Staff",
      title: "No teachers added",
      detail: "Give each teacher an account, then assign their classes and subjects.",
      action: "Add teachers",
      screen: "staff",
    });
  }
  if (summary.currentTerm && summary.marksExpectedThisTerm > 0 && marksPercent < 100) {
    items.push({
      key: "marks",
      tone: "info",
      tag: "Marks",
      title: `${100 - marksPercent}% of marks still to enter`,
      detail: `${(summary.marksExpectedThisTerm - summary.marksEnteredThisTerm).toLocaleString()} marks are missing for ${summary.currentTerm}.`,
      action: "Enter marks",
      screen: "marks",
    });
  }
  return items;
}

function NeedsAttention({
  items,
  onGo,
}: {
  items: AttentionItem[];
  onGo: (screen: ScreenId, params?: Record<string, string>) => void;
}) {
  return (
    <div className="dash-card">
      <div className="dash-card-head">
        <h2 className="dash-card-title">Needs attention</h2>
        {items.length > 0 && (
          <span className="dash-count">
            {items.length} item{items.length === 1 ? "" : "s"}
          </span>
        )}
      </div>

      {items.length === 0 ? (
        <div className="attn-clear">
          <CheckCircle2 size={18} />
          All clear. Nothing needs attention right now.
        </div>
      ) : (
        <div className="attn-list">
          {items.map((item) => (
            <div key={item.key} className={`attn attn-${item.tone}`}>
              <div className="attn-top">
                <span className="attn-tag">
                  <TriangleAlert size={12} />
                  {item.tag}
                </span>
              </div>
              <div className="attn-title">{item.title}</div>
              <p className="attn-detail">{item.detail}</p>
              <button type="button" className="attn-action" onClick={() => onGo(item.screen, item.params)}>
                {item.action}
                <ChevronRight size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recent activity
// ---------------------------------------------------------------------------

function RecentActivity({
  entries,
  onViewAll,
}: {
  entries: DashboardSummary["recentActivity"];
  onViewAll: () => void;
}) {
  return (
    <div className="dash-card">
      <div className="dash-card-head">
        <div>
          <h2 className="dash-card-title">Recent activity</h2>
          <p className="dash-card-sub">Every change is recorded, by whom and when</p>
        </div>
        <Button size="sm" variant="ghost" onClick={onViewAll}>
          View all
        </Button>
      </div>

      {entries.length === 0 ? (
        <EmptyState icon={<TriangleAlert size={18} />} title="Nothing recorded yet">
          Activity appears here as soon as people start using RM.
        </EmptyState>
      ) : (
        <ul className="activity">
          {entries.map((entry) => (
            <li key={entry.id}>
              <span className="activity-avatar">{entry.actor_name ? initials(entry.actor_name) : "RM"}</span>
              <span className="grow">
                <span className="activity-text">{entry.summary}</span>
                <span className="activity-meta">
                  {entry.actor_name ?? "System"} · {relativeTime(entry.at)}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

function DashboardSkeleton() {
  return (
    <div className="page">
      <div className="page-inner dash">
        <div className="skeleton" style={{ height: 30, width: 280 }} />
        <section className="dash-stats">
          {[0, 1, 2, 3].map((index) => (
            <div key={index} className="dash-stat">
              <div className="skeleton" style={{ height: 11, width: "50%" }} />
              <div className="skeleton" style={{ height: 30, width: "35%", marginTop: 14 }} />
            </div>
          ))}
        </section>
        <div className="skeleton" style={{ height: 300, borderRadius: 18 }} />
      </div>
    </div>
  );
}

/** The part of a name a greeting should use, tolerating a missing one. */
function firstName(name: string | null | undefined): string {
  if (typeof name !== "string") return "there";
  return name.trim().split(/\s+/)[0] || "there";
}

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "morning";
  if (hour < 17) return "afternoon";
  return "evening";
}
