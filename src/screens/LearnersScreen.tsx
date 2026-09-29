/**
 * FR-C7 (search by name or registration number), FR-C8 (bio-data, audited),
 * FR-C10 (add and drop — never delete), FR-B8 (the fees block).
 */

import { useCallback, useEffect, useState } from "react";
import { FileSpreadsheet, Search, UserMinus, UserPlus, UserRoundCheck, Users } from "lucide-react";

import { ImportLearnersModal } from "../components/ImportLearnersModal";
import { api } from "../lib/api";
import type { ClassRow, StudentRow } from "../lib/types";
import { useStore } from "../state/store";
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
  TextArea,
  TextInput,
  formatDate,
} from "../components/ui";

export function LearnersScreen() {
  const session = useStore((state) => state.session);
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState("");
  const [roster, setRoster] = useState<StudentRow[]>([]);
  const [includeDropped, setIncludeDropped] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<StudentRow[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [feesRuleOn, setFeesRuleOn] = useState(false);

  const [editing, setEditing] = useState<StudentRow | "new" | null>(null);
  const [dropping, setDropping] = useState<StudentRow | null>(null);
  const [importing, setImporting] = useState(false);

  const isAdmin = session?.isAdmin ?? false;

  const loadRoster = useCallback(async () => {
    if (!classId) return;
    setLoading(true);
    try {
      setRoster(await api.listClassRoster(classId, includeDropped));
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [classId, includeDropped, reportError]);

  useEffect(() => {
    api
      .listClasses()
      .then((loaded) => {
        setClasses(loaded);
        if (loaded.length > 0) setClassId(loaded[0]!.id);
        else setLoading(false);
      })
      .catch(reportError);
    api.getFeesRule().then(setFeesRuleOn).catch(() => undefined);
  }, [reportError]);

  useEffect(() => {
    void loadRoster();
  }, [loadRoster]);

  // FR-C7: searching is institution-wide, but the backend still filters to the
  // classes this teacher is allowed to see.
  useEffect(() => {
    const trimmed = query.trim();
    if (trimmed.length < 2) {
      setResults(null);
      return;
    }
    const handle = window.setTimeout(() => {
      api.searchStudents(trimmed).then(setResults).catch(reportError);
    }, 220);
    return () => window.clearTimeout(handle);
  }, [query, reportError]);

  const shown = results ?? roster;
  const selectedClass = classes.find((entry) => entry.id === classId);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Learners</h1>
            <p className="page-description">
              Learners are added to a class or dropped from it — never deleted.
              A dropped learner keeps every mark and can be readmitted.
            </p>
          </div>
          <div className="page-actions">
            {isAdmin && (
              <Button
                icon={<FileSpreadsheet size={15} />}
                onClick={() => setImporting(true)}
                disabled={classes.length === 0}
              >
                Import spreadsheet
              </Button>
            )}
            <Button
              variant="primary"
              icon={<UserPlus size={15} />}
              onClick={() => setEditing("new")}
              disabled={!classId}
            >
              Add learner
            </Button>
          </div>
        </div>

        <Card>
          <div className="row" style={{ gap: "var(--space-4)", flexWrap: "wrap" }}>
            <div style={{ minWidth: 220, flex: "0 0 auto" }}>
              <SelectInput
                label="Class"
                value={classId}
                onChange={(event) => setClassId(event.target.value)}
              >
                {classes.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.name} ({entry.learnerCount})
                  </option>
                ))}
              </SelectInput>
            </div>

            <div className="grow" style={{ minWidth: 260 }}>
              <div className="field">
                <span className="field-label">Search the whole school</span>
                <div style={{ position: "relative" }}>
                  <Search
                    size={15}
                    style={{
                      position: "absolute",
                      left: 10,
                      top: "50%",
                      transform: "translateY(-50%)",
                      color: "var(--text-tertiary)",
                    }}
                  />
                  <input
                    className="input"
                    style={{ paddingLeft: 32 }}
                    placeholder="Name or registration number"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                  />
                </div>
              </div>
            </div>

            <label className="row" style={{ gap: "var(--space-2)", paddingTop: 18 }}>
              <Switch
                checked={includeDropped}
                onChange={setIncludeDropped}
                label="Show dropped learners"
              />
              <span style={{ fontSize: "var(--text-sm)" }}>Show dropped</span>
            </label>
          </div>
        </Card>

        {results !== null && (
          <Alert tone="info">
            Showing {results.length} search result
            {results.length === 1 ? "" : "s"} for “{query.trim()}”. Clear the
            box to go back to {selectedClass?.name ?? "the class roster"}.
          </Alert>
        )}

        <Card
          title={results !== null ? "Search results" : (selectedClass?.name ?? "Roster")}
          subtitle={
            results === null && selectedClass
              ? `${roster.filter((s) => s.enrollmentStatus === "active").length} active learners`
              : undefined
          }
          flush
        >
          {loading ? (
            <TableSkeleton rows={8} columns={5} />
          ) : shown.length === 0 ? (
            <EmptyState
              icon={<Users size={18} />}
              title={results !== null ? "No learners found" : "No learners in this class yet"}
              action={
                results === null ? (
                  <Button
                    variant="primary"
                    icon={<UserPlus size={15} />}
                    onClick={() => setEditing("new")}
                  >
                    Add the first learner
                  </Button>
                ) : undefined
              }
            >
              {results !== null
                ? "Try part of a name or a registration number."
                : isAdmin
                  ? "Add learners one at a time, or import the school's existing spreadsheet."
                  : "Add learners one at a time."}
            </EmptyState>
          ) : (
            <div className="table-wrap" style={{ maxHeight: "60vh" }}>
              <table className="table">
                <thead>
                  <tr>
                    <th style={{ width: 150 }}>Reg. No.</th>
                    <th>Name</th>
                    <th style={{ width: 70 }} className="center">
                      Sex
                    </th>
                    {results !== null && <th style={{ width: 130 }}>Class</th>}
                    <th style={{ width: 190 }}>Guardian</th>
                    <th style={{ width: 120 }}>Status</th>
                    <th style={{ width: 150 }} />
                  </tr>
                </thead>
                <tbody>
                  {shown.map((student) => (
                    <tr key={student.id}>
                      <td className="mono muted">{student.regNumber}</td>
                      <td style={{ fontWeight: 500 }}>{student.fullName}</td>
                      <td className="center">{student.gender ?? "—"}</td>
                      {results !== null && (
                        <td className="muted">{student.className ?? "—"}</td>
                      )}
                      <td>
                        <div>{student.guardianName ?? "—"}</div>
                        {student.guardianPhone && (
                          <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                            {student.guardianPhone}
                          </div>
                        )}
                      </td>
                      <td>
                        {student.enrollmentStatus === "dropped" ? (
                          <Badge tone="warning">Dropped</Badge>
                        ) : student.status === "graduated" ? (
                          <Badge tone="info">Graduated</Badge>
                        ) : (
                          <Badge tone="success">Active</Badge>
                        )}
                      </td>
                      <td>
                        <div className="row" style={{ gap: 2, justifyContent: "flex-end" }}>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => setEditing(student)}
                          >
                            Edit
                          </Button>
                          {student.enrollmentStatus === "dropped" ? (
                            <Button
                              size="sm"
                              variant="ghost"
                              icon={<UserRoundCheck size={14} />}
                              title="Readmit"
                              onClick={() =>
                                api
                                  .readmitStudent(student.id, classId)
                                  .then(() => {
                                    toast("success", `${student.fullName} readmitted.`);
                                    void loadRoster();
                                  })
                                  .catch(reportError)
                              }
                            />
                          ) : (
                            <Button
                              size="sm"
                              variant="ghost"
                              icon={<UserMinus size={14} />}
                              title="Drop"
                              onClick={() => setDropping(student)}
                            />
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>

        {isAdmin && (
          <Card
            title="Fees block"
            subtitle="FR-B8 — when this is on, learners you flag get no exam permit and no report card"
          >
            <label className="row-between">
              <span>
                <span style={{ fontWeight: 500 }}>
                  Block printing for learners with outstanding fees
                </span>
                <span className="field-hint" style={{ display: "block" }}>
                  Blocked learners are listed after each print run rather than
                  quietly skipped, so nobody is missed by accident.
                </span>
              </span>
              <Switch
                checked={feesRuleOn}
                onChange={(next) => {
                  setFeesRuleOn(next);
                  api
                    .setFeesRule(next)
                    .then(() => toast("success", `Fees block turned ${next ? "on" : "off"}.`))
                    .catch(reportError);
                }}
                label="Fees block rule"
              />
            </label>
          </Card>
        )}
      </div>

      <ImportLearnersModal
        open={importing}
        classes={classes}
        onClose={() => setImporting(false)}
        onImported={() => {
          void loadRoster();
          api.listClasses().then(setClasses).catch(reportError);
        }}
      />

      <StudentModal
        target={editing}
        classId={classId}
        classes={classes}
        onClose={() => setEditing(null)}
        onSaved={() => {
          setEditing(null);
          void loadRoster();
        }}
      />

      <DropModal
        student={dropping}
        onClose={() => setDropping(null)}
        onDropped={() => {
          setDropping(null);
          void loadRoster();
        }}
      />
    </div>
  );
}

function StudentModal({
  target,
  classId,
  classes,
  onClose,
  onSaved,
}: {
  target: StudentRow | "new" | null;
  classId: string;
  classes: ClassRow[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [fullName, setFullName] = useState("");
  const [regNumber, setRegNumber] = useState("");
  const [gender, setGender] = useState("");
  const [dateOfBirth, setDateOfBirth] = useState("");
  const [lin, setLin] = useState("");
  const [guardianName, setGuardianName] = useState("");
  const [guardianPhone, setGuardianPhone] = useState("");
  const [guardianRelationship, setGuardianRelationship] = useState("");
  const [targetClassId, setTargetClassId] = useState(classId);
  const [busy, setBusy] = useState(false);

  const isNew = target === "new";

  useEffect(() => {
    if (!target) return;
    if (target === "new") {
      setFullName("");
      setRegNumber("");
      setGender("");
      setDateOfBirth("");
      setLin("");
      setGuardianName("");
      setGuardianPhone("");
      setGuardianRelationship("");
      setTargetClassId(classId);
    } else {
      setFullName(target.fullName);
      setRegNumber(target.regNumber);
      setGender(target.gender ?? "");
      setDateOfBirth(target.dateOfBirth ?? "");
      setLin(target.lin ?? "");
      setGuardianName(target.guardianName ?? "");
      setGuardianPhone(target.guardianPhone ?? "");
      setGuardianRelationship(target.guardianRelationship ?? "");
    }
  }, [target, classId]);

  if (!target) return null;

  return (
    <Modal
      open
      title={isNew ? "Add a learner" : `Edit ${target.fullName}`}
      description={
        isNew
          ? "Leave the registration number blank and RM will allocate the next one."
          : "Every change here is recorded against your name."
      }
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={fullName.trim().length < 2}
            onClick={() => {
              setBusy(true);
              api
                .saveStudent({
                  id: isNew ? undefined : target.id,
                  fullName,
                  regNumber: regNumber.trim() || null,
                  gender: gender || null,
                  dateOfBirth: dateOfBirth || null,
                  lin: lin.trim() || null,
                  guardianName: guardianName.trim() || null,
                  guardianPhone: guardianPhone.trim() || null,
                  guardianRelationship: guardianRelationship.trim() || null,
                  classId: isNew ? targetClassId : null,
                })
                .then((result) => {
                  toast(
                    "success",
                    isNew
                      ? `${fullName.trim()} added as ${result.regNumber}.`
                      : "Learner updated.",
                  );
                  onSaved();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            {isNew ? "Add learner" : "Save changes"}
          </Button>
        </>
      }
    >
      <div className="grid-form">
        <TextInput
          label="Full name"
          className="span-2"
          value={fullName}
          onChange={(event) => setFullName(event.target.value)}
          autoFocus
        />
        <TextInput
          label="Registration number"
          value={regNumber}
          disabled={!isNew}
          onChange={(event) => setRegNumber(event.target.value)}
          hint={isNew ? "Leave blank to allocate automatically." : "Cannot be changed."}
        />
        {isNew && (
          <SelectInput
            label="Class"
            value={targetClassId}
            onChange={(event) => setTargetClassId(event.target.value)}
          >
            {classes.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.name}
              </option>
            ))}
          </SelectInput>
        )}
        <SelectInput
          label="Sex"
          value={gender}
          onChange={(event) => setGender(event.target.value)}
        >
          <option value="">Not recorded</option>
          <option value="M">Male</option>
          <option value="F">Female</option>
        </SelectInput>
        <TextInput
          label="Date of birth"
          type="date"
          value={dateOfBirth}
          onChange={(event) => setDateOfBirth(event.target.value)}
        />
        <TextInput
          label="LIN"
          value={lin}
          onChange={(event) => setLin(event.target.value)}
          hint="UNEB Learner Identification Number, if issued."
        />
        <TextInput
          label="Guardian's name"
          value={guardianName}
          onChange={(event) => setGuardianName(event.target.value)}
        />
        <TextInput
          label="Guardian's phone"
          value={guardianPhone}
          onChange={(event) => setGuardianPhone(event.target.value)}
        />
        <TextInput
          label="Relationship"
          value={guardianRelationship}
          onChange={(event) => setGuardianRelationship(event.target.value)}
          placeholder="e.g. Mother, Uncle"
        />
      </div>

      {!isNew && target.dateOfBirth && (
        <p className="field-hint" style={{ marginTop: "var(--space-4)" }}>
          Born {formatDate(target.dateOfBirth)}.
        </p>
      )}
    </Modal>
  );
}

function DropModal({
  student,
  onClose,
  onDropped,
}: {
  student: StudentRow | null;
  onClose: () => void;
  onDropped: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (student) setReason("");
  }, [student]);

  if (!student) return null;

  return (
    <Modal
      open
      title={`Drop ${student.fullName}?`}
      description="Nothing is deleted. Their marks, reports and history are kept, and they can be readmitted later."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="danger"
            loading={busy}
            disabled={reason.trim().length < 3}
            onClick={() => {
              setBusy(true);
              api
                .dropStudent(student.id, reason)
                .then(() => {
                  toast("success", `${student.fullName} was dropped.`);
                  onDropped();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Drop learner
          </Button>
        </>
      }
    >
      <TextArea
        label="Reason"
        value={reason}
        onChange={(event) => setReason(event.target.value)}
        placeholder="e.g. Transferred to another school"
        hint="Kept on the record permanently, alongside your name and the date."
        autoFocus
      />
    </Modal>
  );
}
