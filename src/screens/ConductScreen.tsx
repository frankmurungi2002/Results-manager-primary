/**
 * FR-G14: the conduct log. Commendations as well as incidents, what was
 * done about it, and whether the parent was told. Entries are withdrawn
 * with a reason, never deleted.
 */

import { useCallback, useEffect, useState } from "react";
import { Award, Plus, ShieldAlert, Undo2 } from "lucide-react";

import { api } from "../lib/api";
import type { ClassRow, ConductEntry, ConductLog, StudentRow } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { LearnerPicker } from "../components/LearnerPicker";
import { Alert, Badge, Button, Card, EmptyState, Loading, Modal, SelectInput, TextArea, TextInput, cx, formatDate } from "../components/ui";

export function ConductScreen() {
  const can = useCan();
  const session = useStore((state) => state.session);
  const offlineOnly = useStore((state) => state.offlineOnly);
  const reportError = useStore((state) => state.reportError);
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState<string>("");
  const [log, setLog] = useState<ConductLog | null>(null);
  const [adding, setAdding] = useState(false);
  const [withdrawing, setWithdrawing] = useState<ConductEntry | null>(null);
  const [show, setShow] = useState<"all" | "incident" | "commendation">("all");
  const everyone = can("manage_all_classes");

  useEffect(() => {
    api
      .listClasses()
      .then((all) => {
        const mine = everyone ? all : all.filter((c) => session?.classTeacherOf.includes(c.id));
        setClasses(mine.filter((c) => c.status === "active"));
        if (!everyone && mine[0]) setClassId(mine[0].id);
      })
      .catch(reportError);
  }, [everyone, session, reportError]);

  const load = useCallback(() => {
    if (!everyone && !classId) return;
    api.conductLog(classId || null, null).then(setLog).catch(reportError);
  }, [classId, everyone, reportError]);
  useEffect(load, [load]);

  const entries = log ? log.entries.filter((e) => show === "all" || e.kind === show) : [];

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Conduct</h1>
            <p className="page-description">
              Good conduct as well as incidents, what was done, and whether the parent knows. Corporal punishment is banned, so the
              actions offered are corrective.
            </p>
          </div>
          <Button variant="primary" icon={<Plus size={15} />} disabled={!log} onClick={() => setAdding(true)}>
            Record
          </Button>
        </div>
        <div className="row" style={{ gap: 12, alignItems: "end" }}>
          <div style={{ width: 200 }}>
            <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
              {everyone && <option value="">Every class</option>}
              {classes.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </SelectInput>
          </div>
          <div style={{ width: 200 }}>
            <SelectInput label="Show" value={show} onChange={(e) => setShow(e.target.value as typeof show)}>
              <option value="all">Everything</option>
              <option value="incident">Incidents</option>
              <option value="commendation">Commendations</option>
            </SelectInput>
          </div>
        </div>
        {!log ? (
          everyone || classId ? <Loading /> : <EmptyState title="You are not a Class Teacher">The conduct log is kept by Class Teachers and the head.</EmptyState>
        ) : (
          <Card flush>
            {entries.length === 0 ? (
              <EmptyState icon={<Award size={20} />} title="Nothing recorded">Record good conduct as well as incidents.</EmptyState>
            ) : (
              <div className="table-wrap">
                <table className="table table-compact">
                  <thead>
                    <tr>
                      <th>Date</th>
                      <th>Learner</th>
                      <th>What</th>
                      <th>Action</th>
                      <th>Parent</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {entries.map((e) => (
                      <tr key={e.id} className={cx(e.status === "withdrawn" && "is-quiet")}>
                        <td style={{ whiteSpace: "nowrap" }}>{formatDate(e.onDate)}</td>
                        <td>
                          <div style={{ fontWeight: 600 }}>{e.studentName}</div>
                          <div className="subtle">{e.className}</div>
                        </td>
                        <td>
                          <Badge tone={e.kind === "commendation" ? "success" : "warning"}>{e.category}</Badge>
                          <div style={{ marginTop: 4 }}>{e.description}</div>
                          {e.status === "withdrawn" && <div className="subtle">Withdrawn: {e.withdrawnReason}</div>}
                          <div className="subtle">by {e.recordedBy ?? "—"}</div>
                        </td>
                        <td>{e.actionTaken ?? "—"}</td>
                        <td>{e.parentInformed ? "Told" : "—"}</td>
                        <td className="right">
                          {e.status === "active" && (
                            <Button size="sm" variant="ghost" icon={<Undo2 size={14} />} title="Withdraw" onClick={() => setWithdrawing(e)} />
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        )}
      </div>
      {adding && log && (
        <RecordModal
          log={log}
          offlineOnly={offlineOnly}
          onClose={(saved) => {
            setAdding(false);
            if (saved) load();
          }}
        />
      )}
      {withdrawing && (
        <WithdrawModal
          entry={withdrawing}
          onClose={(done) => {
            setWithdrawing(null);
            if (done) load();
          }}
        />
      )}
    </div>
  );
}

function RecordModal({ log, offlineOnly, onClose }: { log: ConductLog; offlineOnly: boolean; onClose: (saved: boolean) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [learner, setLearner] = useState<StudentRow | null>(null);
  const [kind, setKind] = useState<"incident" | "commendation">("incident");
  const [category, setCategory] = useState(log.incidents[0] ?? "");
  const [description, setDescription] = useState("");
  const [action, setAction] = useState("");
  const [told, setTold] = useState(false);
  const [text, setText] = useState(false);
  const [busy, setBusy] = useState(false);
  const list = kind === "incident" ? log.incidents : log.commendations;

  const save = () => {
    if (!learner) return;
    setBusy(true);
    api
      .recordConduct({ studentId: learner.id, onDate: null, kind, category, description, actionTaken: action || null, parentInformed: told, textParent: text })
      .then((saved) => {
        toast("success", saved.texted ? "Recorded, and the parent will get an SMS." : "Recorded.");
        onClose(true);
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      title="Record conduct"
      onClose={() => onClose(false)}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(false)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} disabled={!learner || description.trim().length < 5} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div className="stack">
        <LearnerPicker value={learner} onChange={setLearner} />
        <div className="segmented" role="tablist">
          <button
            className={cx(kind === "incident" && "is-active")}
            onClick={() => {
              setKind("incident");
              setCategory(log.incidents[0] ?? "");
            }}
          >
            <ShieldAlert size={14} /> Incident
          </button>
          <button
            className={cx(kind === "commendation" && "is-active")}
            onClick={() => {
              setKind("commendation");
              setCategory(log.commendations[0] ?? "");
            }}
          >
            <Award size={14} /> Commendation
          </button>
        </div>
        <SelectInput label={kind === "incident" ? "What happened" : "Commended for"} value={category} onChange={(e) => setCategory(e.target.value)}>
          {list.map((c) => (
            <option key={c}>{c}</option>
          ))}
        </SelectInput>
        <TextArea label="In a sentence" rows={2} value={description} onChange={(e) => setDescription(e.target.value)} />
        <SelectInput label="Action taken" value={action} onChange={(e) => setAction(e.target.value)}>
          <option value="">None yet</option>
          {log.actions.map((a) => (
            <option key={a}>{a}</option>
          ))}
        </SelectInput>
        <label className="row" style={{ gap: 8 }}>
          <input type="checkbox" checked={told} onChange={(e) => setTold(e.target.checked)} /> The parent has been told
        </label>
        <label className="row" style={{ gap: 8, opacity: offlineOnly ? 0.5 : 1 }}>
          <input type="checkbox" disabled={offlineOnly} checked={text} onChange={(e) => setText(e.target.checked)} /> Text the parent{" "}
          {offlineOnly && <span className="subtle">(this computer is set to offline only)</span>}
        </label>
        {text && <Alert tone="info">The SMS names the category and your sentence, so write it as the parent should read it.</Alert>}
      </div>
    </Modal>
  );
}

function WithdrawModal({ entry, onClose }: { entry: ConductEntry; onClose: (done: boolean) => void }) {
  const reportError = useStore((state) => state.reportError);
  const [reason, setReason] = useState("");
  return (
    <Modal
      open
      title="Withdraw this entry?"
      description={`${entry.category} for ${entry.studentName} on ${formatDate(entry.onDate)}. It stays on record, marked withdrawn, with your reason.`}
      onClose={() => onClose(false)}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(false)}>
            Keep it
          </Button>
          <Button variant="danger" disabled={reason.trim().length < 5} onClick={() => api.withdrawConduct(entry.id, reason).then(() => onClose(true)).catch(reportError)}>
            Withdraw
          </Button>
        </>
      }
    >
      <TextInput label="Why" placeholder="e.g. Wrong learner" value={reason} onChange={(e) => setReason(e.target.value)} />
    </Modal>
  );
}
