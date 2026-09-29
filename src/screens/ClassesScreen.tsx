/**
 * FR-B5 (classes and the institutional subject catalogue) and
 * FR-C13 (per-class subject management and the grading override).
 */

import { useCallback, useEffect, useState } from "react";
import { BookOpenCheck, Pencil, Plus, Star, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { ClassRow, ClassSubjectRow, GradingSystem, SubjectRow } from "../lib/types";
import { useStore } from "../state/store";
import { StreamsCard } from "../components/StreamsCard";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Modal,
  SelectInput,
  Switch,
  TableSkeleton,
  TextInput,
  cx,
} from "../components/ui";

export function ClassesScreen() {
  const features = useStore((state) => state.features);
  const session = useStore((state) => state.session);
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [classSubjects, setClassSubjects] = useState<ClassSubjectRow[]>([]);
  const [catalogue, setCatalogue] = useState<SubjectRow[]>([]);
  const [systems, setSystems] = useState<GradingSystem[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingSubjects, setLoadingSubjects] = useState(false);

  const [addOpen, setAddOpen] = useState(false);
  const [editing, setEditing] = useState<ClassSubjectRow | null>(null);

  const isAdmin = session?.isAdmin ?? false;

  const loadClasses = useCallback(async () => {
    try {
      const loaded = await api.listClasses();
      setClasses(loaded);
      setSelectedId((current) =>
        loaded.some((entry) => entry.id === current) ? current : (loaded[0]?.id ?? ""),
      );
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  const loadSubjects = useCallback(async () => {
    if (!selectedId) {
      setClassSubjects([]);
      return;
    }
    setLoadingSubjects(true);
    try {
      setClassSubjects(await api.listClassSubjects(selectedId));
    } catch (error) {
      reportError(error);
    } finally {
      setLoadingSubjects(false);
    }
  }, [selectedId, reportError]);

  useEffect(() => {
    void loadClasses();
    api.listGradingSystems().then(setSystems).catch(() => undefined);
    if (isAdmin) api.listSubjects().then(setCatalogue).catch(() => undefined);
  }, [loadClasses, isAdmin]);

  useEffect(() => {
    void loadSubjects();
  }, [loadSubjects]);

  const selected = classes.find((entry) => entry.id === selectedId);

  const available = catalogue.filter(
    (subject) =>
      !classSubjects.some(
        (taught) => taught.subjectId === subject.id && taught.status === "active",
      ),
  );

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Classes &amp; subjects</h1>
            <p className="page-description">
              A class can rename a subject or grade it differently without
              affecting any other class — the subject keeps its identity, so
              past years' records stay intact.
            </p>
          </div>
        </div>

        {loading ? (
          <Card flush>
            <TableSkeleton rows={6} columns={3} />
          </Card>
        ) : classes.length === 0 ? (
          <Card>
            <EmptyState icon={<BookOpenCheck size={18} />} title="No classes yet">
              Classes are created during setup. A School Admin can add more from
              here.
            </EmptyState>
          </Card>
        ) : (
          <div className="split">
            <Card className="list-panel" title="Classes" flush>
              <div className="list-scroll">
                {classes.map((entry) => (
                  <button
                    key={entry.id}
                    className={cx("list-row", entry.id === selectedId && "is-active")}
                    onClick={() => setSelectedId(entry.id)}
                  >
                    <span className="grow">
                      <span className="list-row-title">{entry.name}</span>
                      <span className="list-row-meta" style={{ display: "block" }}>
                        {entry.learnerCount} learners · {entry.subjectCount} subjects
                      </span>
                    </span>
                    <Badge tone={entry.levelKind === "nursery" ? "info" : "neutral"}>
                      {entry.levelKind === "nursery" ? "Nursery" : "Primary"}
                    </Badge>
                  </button>
                ))}
              </div>
            </Card>

            <div className="stack">
              {selected && (
                <Card
                  title={selected.name}
                  subtitle={
                    selected.classTeacherName
                      ? `Class Teacher: ${selected.classTeacherName}`
                      : "No Class Teacher assigned yet"
                  }
                  actions={
                    isAdmin && (
                      <Button
                        size="sm"
                        variant="primary"
                        icon={<Plus size={14} />}
                        onClick={() => setAddOpen(true)}
                        disabled={available.length === 0}
                      >
                        Add subject
                      </Button>
                    )
                  }
                  flush
                >
                  {loadingSubjects ? (
                    <TableSkeleton rows={5} columns={4} />
                  ) : classSubjects.length === 0 ? (
                    <EmptyState icon={<BookOpenCheck size={18} />} title="No subjects yet">
                      Add the subjects this class is taught.
                    </EmptyState>
                  ) : (
                    <div className="table-wrap">
                      <table className="table">
                        <thead>
                          <tr>
                            <th>Subject</th>
                            <th style={{ width: 90 }} className="num">
                              Out of
                            </th>
                            <th style={{ width: 90 }} className="num">
                              Pass
                            </th>
                            <th style={{ width: 200 }}>Grading</th>
                            <th style={{ width: 170 }}>Teacher</th>
                            <th style={{ width: 90 }} />
                          </tr>
                        </thead>
                        <tbody>
                          {classSubjects.map((subject) => (
                            <tr key={subject.id}>
                              <td>
                                <div className="row" style={{ gap: "var(--space-2)" }}>
                                  {subject.isCore && (
                                    <Star
                                      size={13}
                                      className="subtle"
                                      style={{ color: "var(--gold-500)" }}
                                    />
                                  )}
                                  <span style={{ fontWeight: 500 }}>
                                    {subject.displayName}
                                  </span>
                                </div>
                                {subject.displayName !== subject.catalogueName && (
                                  <div
                                    className="subtle"
                                    style={{ fontSize: "var(--text-2xs)" }}
                                  >
                                    Catalogue: {subject.catalogueName}
                                  </div>
                                )}
                              </td>
                              <td className="num">{subject.maxScore}</td>
                              <td className="num">{subject.passMark}</td>
                              <td>
                                <span className="row" style={{ gap: "var(--space-2)" }}>
                                  <span className="muted">{subject.gradingSystemName}</span>
                                  {subject.gradingOverridden && (
                                    <Badge tone="accent">Override</Badge>
                                  )}
                                </span>
                              </td>
                              <td className="muted">{subject.teacherName ?? "—"}</td>
                              <td>
                                <div className="row" style={{ gap: 2 }}>
                                  <Button
                                    size="sm"
                                    variant="ghost"
                                    icon={<Pencil size={14} />}
                                    onClick={() => setEditing(subject)}
                                    title="Edit"
                                  />
                                  <Button
                                    size="sm"
                                    variant="ghost"
                                    icon={<Trash2 size={14} />}
                                    title="Remove from this class"
                                    onClick={() => {
                                      if (
                                        !window.confirm(
                                          `Remove ${subject.displayName} from ${selected.name}? Marks already recorded are kept.`,
                                        )
                                      )
                                        return;
                                      api
                                        .retireClassSubject(subject.id)
                                        .then(() => {
                                          toast("success", "Subject removed.");
                                          void loadSubjects();
                                        })
                                        .catch(reportError);
                                    }}
                                  />
                                </div>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </Card>
              )}

              {features.streams && selected && (
                <StreamsCard classId={selected.id} className={selected.name} />
              )}

              <Alert tone="info" title="Which subjects count toward the aggregate">
                Subjects marked with a star are the core subjects. In Ugandan
                primary that is the four PLE subjects, and the division is only
                worked out when exactly four are marked core. Everything else is
                reported on the card but left out of the aggregate.
              </Alert>
            </div>
          </div>
        )}
      </div>

      <AddSubjectModal
        open={addOpen}
        classId={selectedId}
        available={available}
        onClose={() => setAddOpen(false)}
        onSaved={() => {
          setAddOpen(false);
          void loadSubjects();
          void loadClasses();
        }}
      />

      <EditSubjectModal
        subject={editing}
        systems={systems}
        onClose={() => setEditing(null)}
        onSaved={() => {
          setEditing(null);
          void loadSubjects();
        }}
      />
    </div>
  );
}

function AddSubjectModal({
  open,
  classId,
  available,
  onClose,
  onSaved,
}: {
  open: boolean;
  classId: string;
  available: SubjectRow[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [subjectId, setSubjectId] = useState("");
  const [isCore, setIsCore] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setSubjectId(available[0]?.id ?? "");
      setIsCore(false);
    }
  }, [open, available]);

  return (
    <Modal
      open={open}
      title="Add a subject to this class"
      description="Pick from the institution's catalogue. You can rename it for this class afterwards."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!subjectId}
            onClick={() => {
              setBusy(true);
              api
                .addClassSubject({ classId, subjectId, isCore })
                .then(() => {
                  toast("success", "Subject added.");
                  onSaved();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Add subject
          </Button>
        </>
      }
    >
      <div className="stack">
        <SelectInput
          label="Subject"
          value={subjectId}
          onChange={(event) => setSubjectId(event.target.value)}
        >
          {available.length === 0 && (
            <option value="">Every catalogue subject is already taught here</option>
          )}
          {available.map((subject) => (
            <option key={subject.id} value={subject.id}>
              {subject.name} ({subject.code})
            </option>
          ))}
        </SelectInput>

        <label className="row-between">
          <span>
            <span style={{ fontWeight: 500 }}>Counts toward the aggregate</span>
            <span className="field-hint" style={{ display: "block" }}>
              Turn on for a core subject such as English or Mathematics.
            </span>
          </span>
          <Switch checked={isCore} onChange={setIsCore} label="Core subject" />
        </label>
      </div>
    </Modal>
  );
}

function EditSubjectModal({
  subject,
  systems,
  onClose,
  onSaved,
}: {
  subject: ClassSubjectRow | null;
  systems: GradingSystem[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [displayName, setDisplayName] = useState("");
  const [gradingSystemId, setGradingSystemId] = useState("");
  const [maxScore, setMaxScore] = useState("100");
  const [passMark, setPassMark] = useState("50");
  const [isCore, setIsCore] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!subject) return;
    setDisplayName(
      subject.displayName === subject.catalogueName ? "" : subject.displayName,
    );
    setGradingSystemId(subject.gradingOverridden ? subject.gradingSystemId : "");
    setMaxScore(String(subject.maxScore));
    setPassMark(String(subject.passMark));
    setIsCore(subject.isCore);
  }, [subject]);

  if (!subject) return null;

  return (
    <Modal
      open
      title={`Edit ${subject.catalogueName}`}
      description="These changes apply to this class only."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .updateClassSubject({
                  id: subject.id,
                  displayName: displayName.trim() || null,
                  gradingSystemId: gradingSystemId || null,
                  maxScore: Number(maxScore),
                  passMark: Number(passMark),
                  isCore,
                })
                .then(() => {
                  toast("success", "Subject updated.");
                  onSaved();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save changes
          </Button>
        </>
      }
    >
      <div className="stack">
        <TextInput
          label="Name in this class"
          value={displayName}
          placeholder={subject.catalogueName}
          onChange={(event) => setDisplayName(event.target.value)}
          hint="Leave blank to use the catalogue name. A rename here does not affect other classes or past records."
        />

        <SelectInput
          label="Grading system"
          value={gradingSystemId}
          onChange={(event) => setGradingSystemId(event.target.value)}
          hint="Leave on the class default unless this subject is graded differently."
        >
          <option value="">Use the class default</option>
          {systems.map((system) => (
            <option key={system.id} value={system.id}>
              {system.name}
            </option>
          ))}
        </SelectInput>

        <div className="grid-form">
          <TextInput
            label="Marked out of"
            type="number"
            min={1}
            value={maxScore}
            onChange={(event) => setMaxScore(event.target.value)}
          />
          <TextInput
            label="Pass mark"
            type="number"
            min={0}
            value={passMark}
            onChange={(event) => setPassMark(event.target.value)}
          />
        </div>

        <label className="row-between">
          <span style={{ fontWeight: 500 }}>Counts toward the aggregate</span>
          <Switch checked={isCore} onChange={setIsCore} label="Core subject" />
        </label>
      </div>
    </Modal>
  );
}
