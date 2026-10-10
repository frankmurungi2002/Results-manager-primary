/**
 * Registrations the school makes with outside systems: UNEB (PLE
 * candidates), the Ministry's EMIS census, and UPSSA (school sports).
 *
 * Each tab checks the learners the same way and lets the problem be fixed
 * where it is shown, before anything is typed into a portal: corrections
 * made afterwards cost money or a signed letter.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Camera, Download, FileText, Pencil, Plus, Printer, Trash2, Trophy, UserMinus } from "lucide-react";

import { api } from "../lib/api";
import type {
  ClassRow,
  Correction,
  CorrectionLetter,
  DocumentEnvelope,
  EmisCheck,
  Issue,
  LearnerCheck,
  PleCheck,
  SportsEvent,
  SportsTeam,
  StaffCheck,
  StagedFile,
  StudentRow,
} from "../lib/types";
import { useCan, useStore } from "../state/store";
import { CorrectionLetterSheet, PleRegisterSheet, PleSlipsSheet, PrintPreview, SportsAlbumSheet } from "../components/PrintDocument";
import { PhotoModal } from "../components/PhotoModal";
import { LearnerPicker } from "../components/LearnerPicker";
import { keepExport } from "../components/SaveExport";
import { Alert, Badge, Button, Card, EmptyState, Loading, Modal, SelectInput, StatTile, TextArea, TextInput, cx, formatDate } from "../components/ui";

type Tab = "ple" | "emis" | "sports";
type Printing =
  | { kind: "slips" | "register"; envelope: DocumentEnvelope<PleCheck> }
  | { kind: "album"; envelope: DocumentEnvelope<SportsTeam> }
  | { kind: "letter"; envelope: DocumentEnvelope<CorrectionLetter> };

export function RegistrationsScreen() {
  const can = useCan();
  const [tab, setTab] = useState<Tab>(can("view_all_learners") ? "ple" : "sports");
  const [printing, setPrinting] = useState<Printing | null>(null);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview actions={<Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>Back</Button>}>
          {printing.kind === "slips" && <PleSlipsSheet envelope={printing.envelope} />}
          {printing.kind === "register" && <PleRegisterSheet envelope={printing.envelope} />}
          {printing.kind === "album" && <SportsAlbumSheet envelope={printing.envelope} />}
          {printing.kind === "letter" && <CorrectionLetterSheet envelope={printing.envelope} />}
        </PrintPreview>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Registrations</h1>
            <p className="page-description">
              Check learners before they go onto UNEB, EMIS or the sports portal, and fix problems here. Corrections made after
              registration cost money or a signed letter.
            </p>
          </div>
        </div>
        <div className="tabs">
          {can("view_all_learners") && (
            <>
              <button className={cx("tab", tab === "ple" && "is-active")} onClick={() => setTab("ple")}>
                PLE candidates
              </button>
              <button className={cx("tab", tab === "emis" && "is-active")} onClick={() => setTab("emis")}>
                EMIS census
              </button>
            </>
          )}
          <button className={cx("tab", tab === "sports" && "is-active")} onClick={() => setTab("sports")}>
            Sports (UPSSA)
          </button>
        </div>
        {tab === "ple" && <PleTab onPrint={setPrinting} />}
        {tab === "emis" && <EmisTab />}
        {tab === "sports" && <SportsTab onPrint={setPrinting} />}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

export function IssueList({ issues }: { issues: Issue[] }) {
  if (issues.length === 0) return <Badge tone="success">Ready</Badge>;
  return (
    <ul className="issue-list">
      {issues.map((i, n) => (
        <li key={n} className={i.serious ? "is-serious" : "is-check"}>
          {i.message}
        </li>
      ))}
    </ul>
  );
}

function sexLabel(g: string | null) {
  return g === "M" ? "Male" : g === "F" ? "Female" : "—";
}

/** Learners with their checks, and a Fix button on each row. */
function LearnerTable({
  rows,
  onFix,
  extra,
  showClass = false,
}: {
  rows: LearnerCheck[];
  onFix: ((l: LearnerCheck) => void) | null;
  extra?: (l: LearnerCheck) => React.ReactNode;
  showClass?: boolean;
}) {
  return (
    <div className="table-wrap">
      <table className="table table-compact">
        <thead>
          <tr>
            <th>Name</th>
            {showClass && <th>Class</th>}
            <th>Sex</th>
            <th>Date of birth</th>
            <th>LIN</th>
            <th>Photo</th>
            <th style={{ width: "34%" }}>Check</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((l) => (
            <tr key={l.studentId}>
              <td style={{ fontWeight: 600 }}>
                {l.fullName}
                {l.discipline && <div className="subtle" style={{ fontWeight: 400 }}>{l.discipline}</div>}
              </td>
              {showClass && <td>{l.className}</td>}
              <td>{l.gender ?? "—"}</td>
              <td>
                {l.dateOfBirth ? formatDate(l.dateOfBirth) : "—"}
                {l.age !== null && <div className="subtle">{l.age} yrs</div>}
              </td>
              <td className="mono">{l.lin ?? "—"}</td>
              <td>{l.hasPhoto ? "Yes" : "—"}</td>
              <td>
                <IssueList issues={l.issues} />
              </td>
              <td className="right" style={{ whiteSpace: "nowrap" }}>
                {onFix && (
                  <Button size="sm" variant="ghost" icon={<Pencil size={14} />} onClick={() => onFix(l)}>
                    Fix
                  </Button>
                )}
                {extra?.(l)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function FixLearnerModal({ learner, onClose }: { learner: LearnerCheck; onClose: (changed: boolean) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [name, setName] = useState(learner.fullName);
  const [gender, setGender] = useState(learner.gender ?? "");
  const [dob, setDob] = useState(learner.dateOfBirth ?? "");
  const [lin, setLin] = useState(learner.lin ?? "");
  const [needs, setNeeds] = useState(learner.specialNeeds ?? "");
  const [photo, setPhoto] = useState(false);
  const [busy, setBusy] = useState(false);
  const [photoChanged, setPhotoChanged] = useState(false);

  const save = () => {
    setBusy(true);
    api
      .fixLearnerDetails(learner.studentId, {
        fullName: name,
        gender: gender || null,
        dateOfBirth: dob || null,
        lin: lin || null,
        specialNeeds: needs || null,
      })
      .then(() => {
        toast("success", `Saved ${name.trim()}.`);
        onClose(true);
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <>
      <Modal
        open={!photo}
        title={`Fix ${learner.fullName}`}
        description="Write the details exactly as on the birth certificate. The change is kept in the activity log."
        onClose={() => onClose(photoChanged)}
        footer={
          <>
            <Button variant="ghost" icon={<Camera size={15} />} onClick={() => setPhoto(true)}>
              {learner.hasPhoto ? "Change photo" : "Add photo"}
            </Button>
            <span className="grow" />
            <Button variant="ghost" onClick={() => onClose(photoChanged)}>
              Cancel
            </Button>
            <Button variant="primary" loading={busy} onClick={save}>
              Save
            </Button>
          </>
        }
      >
        <div className="stack">
          <IssueList issues={learner.issues} />
          <TextInput label="Full name" value={name} onChange={(e) => setName(e.target.value)} hint="Surname and other names in full: no initials." />
          <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr" }}>
            <SelectInput label="Sex" value={gender} onChange={(e) => setGender(e.target.value)}>
              <option value="">Not recorded</option>
              <option value="M">Male</option>
              <option value="F">Female</option>
            </SelectInput>
            <TextInput label="Date of birth" type="date" value={dob} onChange={(e) => setDob(e.target.value)} />
          </div>
          <TextInput label="LIN" value={lin} onChange={(e) => setLin(e.target.value.toUpperCase())} hint="The Learner Identification Number from EMIS." />
          <TextInput label="Special needs" value={needs} onChange={(e) => setNeeds(e.target.value)} placeholder="optional, e.g. Low vision" />
        </div>
      </Modal>
      <PhotoModal
        open={photo}
        name={learner.fullName}
        load={() => api.getStudentPhoto(learner.studentId)}
        save={async (png) => {
          await api.setStudentPhoto(learner.studentId, png ?? []);
          setPhotoChanged(true);
        }}
        onClose={() => setPhoto(false)}
      />
    </>
  );
}

async function keepAll(files: StagedFile[], toast: (k: "success" | "error" | "info" | "warning", m: string) => void) {
  for (const f of files) {
    const where = await keepExport(f);
    if (where) toast("success", `Saved ${f.detail} to ${where}`);
  }
}

// ---------------------------------------------------------------------------
// PLE candidates
// ---------------------------------------------------------------------------

function PleTab({ onPrint }: { onPrint: (p: Printing) => void }) {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState<string | null>(null);
  const [check, setCheck] = useState<PleCheck | null>(null);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [fixing, setFixing] = useState<LearnerCheck | null>(null);
  const [returnBy, setReturnBy] = useState("");

  useEffect(() => {
    api
      .listClasses()
      .then((all) => setClasses(all.filter((c) => c.levelKind === "primary" && c.status === "active")))
      .catch(reportError);
  }, [reportError]);

  const load = useCallback(() => {
    api
      .pleCheck(classId)
      .then((c) => {
        setCheck(c);
        if (!classId) setClassId(c.classId);
      })
      .catch(reportError);
  }, [classId, reportError]);
  useEffect(load, [load]);

  if (!check) return <Loading />;
  const rows = onlyProblems ? check.candidates.filter((c) => c.issues.length > 0) : check.candidates;
  const print = (kind: "slips" | "register") =>
    api
      .buildPleDocument(check.classId, [], kind, returnBy || null)
      .then((envelope) => onPrint({ kind, envelope }))
      .catch(reportError);

  return (
    <div className="stack" style={{ marginTop: "var(--space-4)" }}>
      <div className="row" style={{ gap: 12, alignItems: "end", flexWrap: "wrap" }}>
        <div style={{ width: 180 }}>
          <SelectInput label="Candidate class" value={check.classId} onChange={(e) => setClassId(e.target.value)}>
            {classes.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </SelectInput>
        </div>
        <div style={{ width: 200 }}>
          <TextInput label="Slips back by (optional)" type="date" value={returnBy} onChange={(e) => setReturnBy(e.target.value)} />
        </div>
        <span className="grow" />
        <Button icon={<Printer size={15} />} onClick={() => print("slips")} disabled={check.candidates.length === 0}>
          Print checking slips
        </Button>
        <Button icon={<FileText size={15} />} onClick={() => print("register")} disabled={check.candidates.length === 0}>
          Print noticeboard list
        </Button>
      </div>
      <div className="grid grid-stats">
        <StatTile label={`${check.className} candidates for PLE ${check.examYear}`} value={check.candidates.length} />
        <StatTile label="Ready to register" value={check.ready} />
        <StatTile label="To fix first" value={check.candidates.length - check.ready} />
      </div>
      <Alert tone="info">
        Each slip goes home with the candidate: the parent checks every letter of the name, the date of birth and the photo, signs,
        and returns it. Fix what comes back here, then register on the UNEB portal. Ages are worked out for 1 November {check.examYear}.
      </Alert>
      <Card
        flush
        title="Candidates"
        actions={
          <label className="row" style={{ gap: 6, fontSize: 13 }}>
            <input type="checkbox" checked={onlyProblems} onChange={(e) => setOnlyProblems(e.target.checked)} /> Only those with something to check
          </label>
        }
      >
        {rows.length === 0 ? (
          <EmptyState icon={<Trophy size={20} />} title={check.candidates.length ? "Every candidate is ready" : "No learners in this class"}>
            {check.candidates.length ? "Nothing to fix." : "Choose the candidate class."}
          </EmptyState>
        ) : (
          <LearnerTable rows={rows} onFix={can("manage_learners") ? setFixing : null} />
        )}
      </Card>
      {fixing && (
        <FixLearnerModal
          learner={fixing}
          onClose={(changed) => {
            setFixing(null);
            if (changed) load();
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// EMIS
// ---------------------------------------------------------------------------

function EmisTab() {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [check, setCheck] = useState<EmisCheck | null>(null);
  const [fixing, setFixing] = useState<LearnerCheck | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.emisCheck().then(setCheck).catch(reportError);
  }, [reportError]);
  useEffect(load, [load]);

  if (!check) return <Loading />;
  const total = check.classes.reduce((n, c) => n + c.boys + c.girls + c.unknownSex, 0);
  const staffReady = check.staff.filter((s) => !s.issues.some((i) => i.serious)).length;

  return (
    <div className="stack" style={{ marginTop: "var(--space-4)" }}>
      <div className="row" style={{ alignItems: "center" }}>
        <p className="subtle grow" style={{ margin: 0 }}>
          The numbers the annual school census asks for, counted from today&apos;s records ({formatDate(check.censusDate)}). Fix
          what is missing, then export the workbook and copy it into the Ministry&apos;s EMIS system.
        </p>
        <Button
          variant="primary"
          icon={<Download size={15} />}
          loading={busy}
          onClick={() => {
            setBusy(true);
            api
              .exportEmis()
              .then((f) => keepAll([f], toast))
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          Export to Excel
        </Button>
      </div>
      <div className="grid grid-stats">
        <StatTile label="Learners" value={total} />
        <StatTile label="Learners with every detail" value={`${check.learnersReady} of ${check.learnersTotal}`} />
        <StatTile label="Staff with sex and NIN" value={`${staffReady} of ${check.staff.length}`} />
      </div>
      <Card flush title={`Enrolment ${check.yearLabel}`}>
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th>Class</th>
                <th className="num">Boys</th>
                <th className="num">Girls</th>
                <th className="num">Total</th>
                <th className="num">New</th>
                <th className="num">Repeating</th>
                <th className="num">Special needs</th>
                <th className="num">Sex / age unknown</th>
              </tr>
            </thead>
            <tbody>
              {check.classes.map((c) => (
                <tr key={c.className}>
                  <td style={{ fontWeight: 600 }}>{c.className}</td>
                  <td className="num">{c.boys}</td>
                  <td className="num">{c.girls}</td>
                  <td className="num">{c.boys + c.girls + c.unknownSex}</td>
                  <td className="num">{c.newBoys + c.newGirls}</td>
                  <td className="num">{c.repeatBoys + c.repeatGirls}</td>
                  <td className="num">{c.specialNeeds}</td>
                  <td className="num">
                    {c.unknownSex || c.unknownAge ? (
                      <span style={{ color: "var(--warning)" }}>
                        {c.unknownSex} / {c.unknownAge}
                      </span>
                    ) : (
                      "—"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>
      <Card flush title="Staff" subtitle="Sex, National ID number and TMIS number for the staff return.">
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th>Name</th>
                <th>Sex</th>
                <th>NIN</th>
                <th>TMIS number</th>
                <th style={{ width: "30%" }}>Check</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {check.staff.map((s) => (
                <StaffRow key={s.userId} staff={s} editable={can("manage_staff")} onSaved={load} />
              ))}
            </tbody>
          </table>
        </div>
      </Card>
      <Card flush title={`Learners to complete (${check.learners.length})`} subtitle="Missing LINs, dates of birth and sex.">
        {check.learners.length === 0 ? (
          <EmptyState title="Every learner is complete">Nothing to fix.</EmptyState>
        ) : (
          <LearnerTable rows={check.learners} showClass onFix={can("manage_learners") ? setFixing : null} />
        )}
      </Card>
      {fixing && (
        <FixLearnerModal
          learner={fixing}
          onClose={(changed) => {
            setFixing(null);
            if (changed) load();
          }}
        />
      )}
    </div>
  );
}

function StaffRow({ staff, editable, onSaved }: { staff: StaffCheck; editable: boolean; onSaved: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [gender, setGender] = useState(staff.gender ?? "");
  const [nin, setNin] = useState(staff.nin ?? "");
  const [tmis, setTmis] = useState(staff.tmisNumber ?? "");
  const dirty = gender !== (staff.gender ?? "") || nin !== (staff.nin ?? "") || tmis !== (staff.tmisNumber ?? "");
  return (
    <tr>
      <td style={{ fontWeight: 600 }}>{staff.fullName}</td>
      <td style={{ width: 110 }}>
        {editable ? (
          <select className="input input-sm" value={gender} onChange={(e) => setGender(e.target.value)} aria-label="Sex">
            <option value="">—</option>
            <option value="M">Male</option>
            <option value="F">Female</option>
          </select>
        ) : (
          sexLabel(staff.gender)
        )}
      </td>
      <td style={{ width: 190 }}>
        {editable ? (
          <input className="input input-sm mono" value={nin} maxLength={16} onChange={(e) => setNin(e.target.value.toUpperCase())} aria-label="NIN" placeholder="CM…" />
        ) : (
          staff.nin ?? "—"
        )}
      </td>
      <td style={{ width: 150 }}>
        {editable ? <input className="input input-sm" value={tmis} onChange={(e) => setTmis(e.target.value)} aria-label="TMIS number" /> : staff.tmisNumber ?? "—"}
      </td>
      <td>
        <IssueList issues={staff.issues} />
      </td>
      <td className="right">
        {editable && dirty && (
          <Button
            size="sm"
            variant="primary"
            onClick={() =>
              api
                .saveStaffIds(staff.userId, { gender: gender || null, nin: nin || null, tmisNumber: tmis || null })
                .then(onSaved)
                .catch(reportError)
            }
          >
            Save
          </Button>
        )}
      </td>
    </tr>
  );
}

// ---------------------------------------------------------------------------
// Sports (UPSSA)
// ---------------------------------------------------------------------------

function SportsTab({ onPrint }: { onPrint: (p: Printing) => void }) {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [events, setEvents] = useState<SportsEvent[] | null>(null);
  const [eventId, setEventId] = useState<string | null>(null);
  const [team, setTeam] = useState<SportsTeam | null>(null);
  const [editing, setEditing] = useState<SportsEvent | "new" | null>(null);
  const [adding, setAdding] = useState<StudentRow | null>(null);
  const [discipline, setDiscipline] = useState("");
  const [pickerKey, setPickerKey] = useState(0);
  const [fixing, setFixing] = useState<LearnerCheck | null>(null);
  const [letter, setLetter] = useState(false);
  const [busy, setBusy] = useState(false);
  const mayRun = can("manage_learners") || can("manage_academics") || can("manage_all_classes");

  const loadEvents = useCallback(() => {
    api
      .listSportsEvents()
      .then((list) => {
        setEvents(list);
        setEventId((current) => (current && list.some((e) => e.id === current) ? current : list[0]?.id ?? null));
      })
      .catch(reportError);
  }, [reportError]);
  useEffect(loadEvents, [loadEvents]);

  const loadTeam = useCallback(() => {
    if (!eventId) {
      setTeam(null);
      return;
    }
    api.sportsTeam(eventId).then(setTeam).catch(reportError);
  }, [eventId, reportError]);
  useEffect(loadTeam, [loadTeam]);

  if (!events) return <Loading />;

  const add = () => {
    if (!adding || !eventId) return;
    api
      .setAthletes(eventId, [adding.id], discipline.trim() || null, true)
      .then(() => {
        setAdding(null);
        setDiscipline("");
        setPickerKey((k) => k + 1);
        loadTeam();
        loadEvents();
      })
      .catch(reportError);
  };

  return (
    <div className="stack" style={{ marginTop: "var(--space-4)" }}>
      <div className="row" style={{ gap: 12, alignItems: "end", flexWrap: "wrap" }}>
        {events.length > 0 && (
          <div style={{ width: 320 }}>
            <SelectInput label="Championship" value={eventId ?? ""} onChange={(e) => setEventId(e.target.value)}>
              {events.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.name}: {e.sport} ({e.athletes})
                </option>
              ))}
            </SelectInput>
          </div>
        )}
        {mayRun && (
          <Button icon={<Plus size={15} />} onClick={() => setEditing("new")}>
            New championship
          </Button>
        )}
        {team && mayRun && (
          <Button variant="ghost" icon={<Pencil size={15} />} onClick={() => setEditing(team.event)}>
            Edit
          </Button>
        )}
        <span className="grow" />
        {team && team.athletes.length > 0 && (
          <>
            <Button icon={<Printer size={15} />} onClick={() => api.buildSportsAlbum(team.event.id).then((envelope) => onPrint({ kind: "album", envelope })).catch(reportError)}>
              Print draft album
            </Button>
            {mayRun && (
              <>
                <Button
                  icon={<Download size={15} />}
                  loading={busy}
                  onClick={() => {
                    setBusy(true);
                    api
                      .exportSportsTeam(team.event.id)
                      .then((files) => keepAll(files, toast))
                      .catch(reportError)
                      .finally(() => setBusy(false));
                  }}
                >
                  Export for the portal
                </Button>
                <Button icon={<FileText size={15} />} onClick={() => setLetter(true)}>
                  Correction letter
                </Button>
              </>
            )}
          </>
        )}
      </div>

      {events.length === 0 ? (
        <Card>
          <EmptyState icon={<Trophy size={20} />} title="No championships yet">
            Set up a championship (for example Ball games, Athletics or Music Dance and Drama) with its age cut-off, then pick the team.
          </EmptyState>
        </Card>
      ) : !team ? (
        <Loading />
      ) : (
        <>
          <div className="grid grid-stats">
            <StatTile
              label="Team"
              value={team.athletes.length}
              meta={team.event.maxAthletes ? `of ${team.event.maxAthletes} allowed` : undefined}
            />
            <StatTile label="Eligible and complete" value={team.ready} />
            <StatTile
              label={team.event.bornOnOrAfter ? "Born on or after" : "Age limit"}
              value={team.event.bornOnOrAfter ? formatDate(team.event.bornOnOrAfter) : "None set"}
              meta={team.event.sex === "M" ? "Boys only" : team.event.sex === "F" ? "Girls only" : "Boys and girls"}
            />
          </div>
          {team.warnings.map((w) => (
            <Alert key={w} tone="warning">
              {w}
            </Alert>
          ))}
          <Alert tone="info">
            On upssaonline.com, register each athlete with the details and photo exported here, then Generate Album. Print the
            draft album first and check it with the class teachers: once the album is generated, a correction needs a letter on the
            school&apos;s headed paper, signed by the headteacher.
          </Alert>
          {mayRun && (
            <Card title="Add to the team">
              <div className="grid-form" style={{ gridTemplateColumns: "2fr 1fr auto", alignItems: "end" }}>
                <LearnerPicker key={pickerKey} value={adding} onChange={setAdding} />
                <TextInput label="Event (optional)" placeholder="e.g. 100 m, Goal shooter" value={discipline} onChange={(e) => setDiscipline(e.target.value)} />
                <Button variant="primary" icon={<Plus size={15} />} disabled={!adding} onClick={add}>
                  Add
                </Button>
              </div>
            </Card>
          )}
          <Card flush title="Athletes">
            {team.athletes.length === 0 ? (
              <EmptyState title="Nobody on the team yet">Add learners above.</EmptyState>
            ) : (
              <LearnerTable
                rows={team.athletes}
                showClass
                onFix={can("manage_learners") ? setFixing : null}
                extra={
                  mayRun
                    ? (l) => (
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<UserMinus size={14} />}
                          title="Take off the team"
                          onClick={() =>
                            api
                              .setAthletes(team.event.id, [l.studentId], null, false)
                              .then(() => {
                                loadTeam();
                                loadEvents();
                              })
                              .catch(reportError)
                          }
                        />
                      )
                    : undefined
                }
              />
            )}
          </Card>
        </>
      )}
      {editing && (
        <EventModal
          event={editing === "new" ? null : editing}
          onClose={(id) => {
            setEditing(null);
            if (id) {
              setEventId(id);
              loadEvents();
              loadTeam();
            }
          }}
        />
      )}
      {fixing && (
        <FixLearnerModal
          learner={fixing}
          onClose={(changed) => {
            setFixing(null);
            if (changed) loadTeam();
          }}
        />
      )}
      {letter && team && (
        <LetterModal
          team={team}
          onClose={() => setLetter(false)}
          onPrint={(envelope) => {
            setLetter(false);
            onPrint({ kind: "letter", envelope });
          }}
        />
      )}
    </div>
  );
}

function EventModal({ event, onClose }: { event: SportsEvent | null; onClose: (savedId: string | null) => void }) {
  const reportError = useStore((state) => state.reportError);
  const [name, setName] = useState(event?.name ?? "");
  const [sport, setSport] = useState(event?.sport ?? "");
  const [cut, setCut] = useState(event?.bornOnOrAfter ?? "");
  const [sex, setSex] = useState(event?.sex ?? "any");
  const [max, setMax] = useState(event?.maxAthletes ? String(event.maxAthletes) : "");
  const [busy, setBusy] = useState(false);

  const save = () => {
    setBusy(true);
    api
      .saveSportsEvent({ id: event?.id ?? null, name, sport, bornOnOrAfter: cut || null, sex, maxAthletes: max ? Number(max) : null })
      .then(onClose)
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      title={event ? "Edit championship" : "New championship"}
      description="The age rule comes from the championship's circular: athletes born before the cut-off are too old."
      onClose={() => onClose(null)}
      footer={
        <>
          {event && (
            <Button
              variant="ghost"
              icon={<Trash2 size={15} />}
              onClick={() =>
                api
                  .retireSportsEvent(event.id)
                  .then(() => onClose(""))
                  .catch(reportError)
              }
            >
              Close championship
            </Button>
          )}
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(null)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} disabled={!name.trim() || !sport.trim()} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div className="stack">
        <TextInput label="Championship" placeholder="e.g. Ball games 2026, zone level" value={name} onChange={(e) => setName(e.target.value)} />
        <TextInput label="Sport" placeholder="e.g. Netball, Football, Athletics" value={sport} onChange={(e) => setSport(e.target.value)} />
        <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr 1fr" }}>
          <TextInput label="Born on or after" type="date" value={cut} onChange={(e) => setCut(e.target.value)} />
          <SelectInput label="For" value={sex} onChange={(e) => setSex(e.target.value)}>
            <option value="any">Boys and girls</option>
            <option value="M">Boys</option>
            <option value="F">Girls</option>
          </SelectInput>
          <TextInput label="Team size limit" type="number" min={1} placeholder="optional" value={max} onChange={(e) => setMax(e.target.value)} />
        </div>
      </div>
    </Modal>
  );
}

const CORRECTABLE = ["Name", "Date of birth", "Sex", "Photo", "LIN", "Class", "Event"];

function LetterModal({ team, onClose, onPrint }: { team: SportsTeam; onClose: () => void; onPrint: (e: DocumentEnvelope<CorrectionLetter>) => void }) {
  const reportError = useStore((state) => state.reportError);
  const [addressee, setAddressee] = useState("The General Secretary\nUganda Primary Schools Sports Association");
  const [rows, setRows] = useState<Correction[]>([{ studentId: team.athletes[0]?.studentId ?? "", what: "Name", wrong: "", right: team.athletes[0]?.fullName ?? "" }]);
  const [note, setNote] = useState("");

  const current = (c: Correction) => {
    const a = team.athletes.find((x) => x.studentId === c.studentId);
    if (!a) return "";
    switch (c.what) {
      case "Name":
        return a.fullName;
      case "Date of birth":
        return a.dateOfBirth ? formatDate(a.dateOfBirth) : "";
      case "Sex":
        return sexLabel(a.gender);
      case "LIN":
        return a.lin ?? "";
      case "Class":
        return a.className;
      case "Event":
        return a.discipline ?? "";
      default:
        return "Attached";
    }
  };
  const update = (i: number, patch: Partial<Correction>) =>
    setRows((rs) =>
      rs.map((r, n) => {
        if (n !== i) return r;
        const next = { ...r, ...patch };
        // The right value defaults to what Termbook has now (already fixed here).
        if (patch.studentId !== undefined || patch.what !== undefined) next.right = current(next);
        return next;
      }),
    );

  return (
    <Modal
      open
      wide
      title="Correction letter"
      description="For details that were wrong on the portal or the album. Fix the learner here first: the correct value is filled in from Termbook."
      onClose={onClose}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            icon={<Printer size={15} />}
            onClick={() =>
              api
                .buildCorrectionLetter({ eventId: team.event.id, addressee, corrections: rows, note: note || null })
                .then(onPrint)
                .catch(reportError)
            }
          >
            Print letter
          </Button>
        </>
      }
    >
      <div className="stack">
        <TextArea label="To" rows={3} value={addressee} onChange={(e) => setAddressee(e.target.value)} />
        {rows.map((r, i) => (
          <div key={i} className="grid-form" style={{ gridTemplateColumns: "1.6fr 1fr 1.2fr 1.2fr auto", alignItems: "end" }}>
            <SelectInput label="Athlete" value={r.studentId} onChange={(e) => update(i, { studentId: e.target.value })}>
              {team.athletes.map((a) => (
                <option key={a.studentId} value={a.studentId}>
                  {a.fullName}
                </option>
              ))}
            </SelectInput>
            <SelectInput label="What" value={r.what} onChange={(e) => update(i, { what: e.target.value })}>
              {CORRECTABLE.map((w) => (
                <option key={w}>{w}</option>
              ))}
            </SelectInput>
            <TextInput label="Shows wrongly as" value={r.wrong} onChange={(e) => update(i, { wrong: e.target.value })} />
            <TextInput label="Should read" value={r.right} onChange={(e) => update(i, { right: e.target.value })} />
            <Button variant="ghost" icon={<Trash2 size={14} />} disabled={rows.length === 1} onClick={() => setRows((rs) => rs.filter((_, n) => n !== i))} />
          </div>
        ))}
        <div>
          <Button
            size="sm"
            variant="ghost"
            icon={<Plus size={14} />}
            onClick={() => {
              const first = team.athletes[0];
              setRows((rs) => [...rs, { studentId: first?.studentId ?? "", what: "Name", wrong: "", right: first?.fullName ?? "" }]);
            }}
          >
            Another correction
          </Button>
        </div>
        <TextArea label="Anything to add (optional)" rows={2} value={note} onChange={(e) => setNote(e.target.value)} placeholder="e.g. Copies of the birth certificates are attached." />
      </div>
    </Modal>
  );
}
