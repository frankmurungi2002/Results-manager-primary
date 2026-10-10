/**
 * Insights: where the P7 class is heading at PLE, who is closest to the next
 * division and what it would take, and which subjects in which classes are
 * behind the school.
 *
 * The projector view is for the staff meeting: the same figures, large
 * enough to read from the back of the staffroom.
 */

import { useCallback, useEffect, useState } from "react";
import { Maximize2, Minimize2, Presentation, Target } from "lucide-react";

import { api } from "../lib/api";
import type { AcademicYearRow, ClassRow, HeatCell, NearMiss, PleProjection, SubjectHeatmap } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { Alert, Badge, Button, Card, EmptyState, Loading, SelectInput, StatTile, cx } from "../components/ui";

type Tab = "ple" | "heat";

export function InsightsScreen() {
  const can = useCan();
  const session = useStore((state) => state.session);
  const reportError = useStore((state) => state.reportError);
  const [tab, setTab] = useState<Tab>("ple");
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [classId, setClassId] = useState("");
  const [termId, setTermId] = useState("");

  useEffect(() => {
    Promise.all([api.listClasses(), api.listAcademicYears()])
      .then(([cls, yrs]) => {
        const mine = can("view_all_classes") || can("view_insights") ? cls : cls.filter((c) => session?.classIds.includes(c.id));
        const primary = mine.filter((c) => c.levelKind === "primary" && c.status === "active");
        setClasses(primary);
        setYears(yrs);
        const top = [...primary].sort((a, b) => b.ladderPosition - a.ladderPosition)[0];
        if (top) setClassId(top.id);
        const terms = yrs.flatMap((y) => y.terms);
        const open = terms.find((t) => t.status === "open") ?? terms.find((t) => t.status === "closed");
        if (open) setTermId(open.id);
      })
      .catch(reportError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportError]);

  const terms = years.flatMap((y) => y.terms.filter((t) => t.status !== "planning").map((t) => ({ ...t, label: `${t.name} ${y.label}` })));

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Insights</h1>
            <p className="page-description">Where the candidates are heading at PLE, and which subjects need help, worked out from the marks entered.</p>
          </div>
        </div>
        <div className="row" style={{ gap: 12, alignItems: "end", flexWrap: "wrap" }}>
          <div className="tabs" style={{ margin: 0 }}>
            <button className={cx("tab", tab === "ple" && "is-active")} onClick={() => setTab("ple")}>
              PLE projection
            </button>
            {can("view_insights") && (
              <button className={cx("tab", tab === "heat" && "is-active")} onClick={() => setTab("heat")}>
                Subject heat map
              </button>
            )}
          </div>
          <span className="grow" />
          {tab === "ple" && (
            <div style={{ width: 160 }}>
              <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
                {classes.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </SelectInput>
            </div>
          )}
          <div style={{ width: 200 }}>
            <SelectInput label="Term" value={termId} onChange={(e) => setTermId(e.target.value)}>
              {terms.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </SelectInput>
          </div>
        </div>
        {!termId ? (
          <Loading />
        ) : tab === "ple" ? (
          classId ? <PleTab classId={classId} termId={termId} /> : <EmptyState title="No primary classes">Add the classes first.</EmptyState>
        ) : (
          <HeatTab termId={termId} />
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// PLE projection
// ---------------------------------------------------------------------------

const DIVISION_TONES = ["div-1", "div-2", "div-3", "div-4", "div-u", "div-x"];

function DivisionBars({ projection, big = false }: { projection: PleProjection; big?: boolean }) {
  const max = Math.max(1, ...projection.divisions.map((d) => d.count));
  return (
    <div className={cx("div-bars", big && "div-bars-big")}>
      {projection.divisions.map((d, i) => (
        <div key={d.label} className="div-bar">
          <div className="div-bar-count">{d.count}</div>
          <div className="div-bar-track">
            <div className={cx("div-bar-fill", DIVISION_TONES[i] ?? "div-x")} style={{ height: `${(d.count / max) * 100}%` }} />
          </div>
          <div className="div-bar-label">{d.label}</div>
          <div className="div-bar-share">{projection.projectedLearners ? Math.round((d.count / projection.projectedLearners) * 100) : 0}%</div>
        </div>
      ))}
    </div>
  );
}

function stepsText(m: NearMiss): string {
  return m.steps.map((s) => `+${Math.ceil(s.marksNeeded)} in ${s.subjectName} (${s.currentGrade}→${s.nextGrade})`).join(", ");
}

function PleTab({ classId, termId }: { classId: string; termId: string }) {
  const reportError = useStore((state) => state.reportError);
  const [projection, setProjection] = useState<PleProjection | null>(null);
  const [projector, setProjector] = useState(false);

  const load = useCallback(() => {
    setProjection(null);
    api.pleProjection(classId, termId).then(setProjection).catch(reportError);
  }, [classId, termId, reportError]);
  useEffect(load, [load]);

  if (!projection) return <Loading />;
  if (!projection.divisionsComputable) {
    return (
      <Alert tone="warning" title="Divisions cannot be worked out for this class">
        PLE divisions come from exactly four core subjects (English, Mathematics, Science and Social Studies). This class has{" "}
        {projection.coreSubjects.length}: {projection.coreSubjects.join(", ") || "none"}. Mark the four core subjects under Classes &amp; subjects.
      </Alert>
    );
  }
  const div1 = projection.divisions[0]?.count ?? 0;

  return (
    <div className="stack" style={{ marginTop: "var(--space-4)" }}>
      <div className="row">
        <p className="subtle grow" style={{ margin: 0 }}>
          {projection.className}, {projection.termName} {projection.academicYear}: each learner&apos;s aggregate from the four core subjects as if this
          term&apos;s marks were PLE.
        </p>
        <Button variant="primary" icon={<Presentation size={15} />} onClick={() => setProjector(true)}>
          Projector view
        </Button>
      </div>
      <div className="grid grid-stats">
        <StatTile label="Projected" value={`${projection.projectedLearners} of ${projection.totalLearners}`} meta={projection.incomplete.length ? `${projection.incomplete.length} missing a core mark` : "Every learner has all four"} />
        <StatTile label="Division 1" value={div1} meta={projection.projectedLearners ? `${Math.round((div1 / projection.projectedLearners) * 100)}% of the class` : undefined} />
        <StatTile label="Best aggregate" value={projection.bestAggregate ?? "—"} meta={projection.meanAggregate !== null ? `Class mean ${projection.meanAggregate.toFixed(1)}` : undefined} />
      </div>
      <Card title="Divisions">
        <DivisionBars projection={projection} />
      </Card>
      <Card flush title="Closest to the next division" subtitle="Cheapest first: the fewest extra marks in the subjects where a grade boundary is nearest.">
        {projection.nearMisses.length === 0 ? (
          <EmptyState icon={<Target size={20} />} title="Nobody is close to a boundary">Every learner is comfortably inside their division.</EmptyState>
        ) : (
          <div className="table-wrap">
            <table className="table table-compact">
              <thead>
                <tr>
                  <th>Learner</th>
                  <th className="num">Aggregate</th>
                  <th>Now</th>
                  <th>Could reach</th>
                  <th>What it takes</th>
                </tr>
              </thead>
              <tbody>
                {projection.nearMisses.map((m) => (
                  <tr key={m.studentId}>
                    <td style={{ fontWeight: 600 }}>{m.fullName}</td>
                    <td className="num">{m.aggregate}</td>
                    <td>{m.currentDivision}</td>
                    <td>
                      <Badge tone="success">{m.targetDivision}</Badge>
                    </td>
                    <td>{stepsText(m)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
      {projection.incomplete.length > 0 && (
        <Card flush title="Not projected" subtitle="A division needs a mark in each core subject.">
          <div className="table-wrap">
            <table className="table table-compact">
              <tbody>
                {projection.incomplete.map((l) => (
                  <tr key={l.studentId}>
                    <td style={{ fontWeight: 600 }}>{l.fullName}</td>
                    <td className="subtle">No mark in {l.missing.join(", ")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      )}
      {projector && <Projector projection={projection} onClose={() => setProjector(false)} />}
    </div>
  );
}

/** Full screen, large type, dark: readable on a classroom projector. */
function Projector({ projection, onClose }: { projection: PleProjection; onClose: () => void }) {
  const institution = useStore((state) => state.institution);
  const [full, setFull] = useState(false);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.fullscreenElement) onClose();
    };
    const change = () => setFull(Boolean(document.fullscreenElement));
    document.addEventListener("keydown", key);
    document.addEventListener("fullscreenchange", change);
    return () => {
      document.removeEventListener("keydown", key);
      document.removeEventListener("fullscreenchange", change);
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    };
  }, [onClose]);

  const toggle = () => {
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    else void document.documentElement.requestFullscreen().catch(() => undefined);
  };
  const div1 = projection.divisions[0]?.count ?? 0;

  return (
    <div className="projector" role="dialog" aria-label="Projector view">
      <div className="projector-head">
        <div>
          <div className="projector-school">{institution?.name}</div>
          <h1>
            {projection.className} PLE projection <span>{projection.termName} {projection.academicYear}</span>
          </h1>
        </div>
        <div className="row" style={{ gap: 8 }}>
          <Button variant="ghost" icon={full ? <Minimize2 size={18} /> : <Maximize2 size={18} />} onClick={toggle}>
            {full ? "Leave full screen" : "Full screen"}
          </Button>
          <Button variant="ghost" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
      <div className="projector-body">
        <div className="projector-stats">
          <div>
            <strong>{projection.projectedLearners}</strong>
            <span>candidates projected</span>
          </div>
          <div>
            <strong>{div1}</strong>
            <span>in Division 1</span>
          </div>
          <div>
            <strong>{projection.meanAggregate?.toFixed(1) ?? "—"}</strong>
            <span>mean aggregate</span>
          </div>
        </div>
        <DivisionBars projection={projection} big />
        {projection.nearMisses.length > 0 && (
          <div className="projector-near">
            <h2>Within reach of the next division</h2>
            <ol>
              {projection.nearMisses.slice(0, 6).map((m) => (
                <li key={m.studentId}>
                  <strong>{m.fullName}</strong> {m.currentDivision} → {m.targetDivision}: {stepsText(m)}
                </li>
              ))}
            </ol>
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Heat map
// ---------------------------------------------------------------------------

function heatColour(delta: number): string {
  // Red below the school mean, green above, white near it; 20 points = full.
  const t = Math.max(-1, Math.min(1, delta / 20));
  return t < 0 ? `color-mix(in srgb, var(--heat-low) ${Math.round(-t * 85)}%, transparent)` : `color-mix(in srgb, var(--heat-high) ${Math.round(t * 85)}%, transparent)`;
}

function HeatTab({ termId }: { termId: string }) {
  const reportError = useStore((state) => state.reportError);
  const [map, setMap] = useState<SubjectHeatmap | null>(null);

  useEffect(() => {
    setMap(null);
    api.subjectHeatmap(termId).then(setMap).catch(reportError);
  }, [termId, reportError]);

  if (!map) return <Loading />;
  if (map.cells.length === 0) {
    return <EmptyState title="No marks this term yet">The heat map fills in as marks are entered.</EmptyState>;
  }
  const cell = (cls: string, subject: string): HeatCell | undefined => map.cells.find((c) => c.className === cls && c.subjectName === subject);

  return (
    <div className="stack" style={{ marginTop: "var(--space-4)" }}>
      <p className="subtle" style={{ margin: 0 }}>
        The average of every mark recorded this term, as a percentage of each paper&apos;s maximum, against the school average of{" "}
        <strong>{map.schoolMean?.toFixed(1) ?? "—"}%</strong>. Red is behind the school, green ahead. It measures the teaching, not any one learner.
      </p>
      <Card flush>
        <div className="table-wrap">
          <table className="table heat-table">
            <thead>
              <tr>
                <th>Class</th>
                {map.subjects.map((s) => (
                  <th key={s} className="center">
                    {s}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {map.classes.map((cls) => (
                <tr key={cls}>
                  <td style={{ fontWeight: 600 }}>{cls}</td>
                  {map.subjects.map((s) => {
                    const c = cell(cls, s);
                    return (
                      <td key={s} className="center heat-cell" style={c ? { background: heatColour(c.delta) } : undefined} title={c ? `${c.learners} learners, ${c.delta >= 0 ? "+" : ""}${c.delta.toFixed(1)} against the school` : "No marks"}>
                        {c ? `${Math.round(c.meanPercentage)}%` : "—"}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>
      {map.weakest.length > 0 && (
        <Card title="Where to look first">
          <ol className="heat-weakest">
            {map.weakest.slice(0, 5).map((c) => (
              <li key={`${c.className}-${c.subjectName}`}>
                <strong>
                  {c.className} {c.subjectName}
                </strong>{" "}
                {Math.round(c.meanPercentage)}%, {Math.abs(c.delta).toFixed(1)} points below the school ({c.learners} learners)
              </li>
            ))}
          </ol>
        </Card>
      )}
    </div>
  );
}
