/**
 * FR-G2: the visitors' book. Who is on the compound right now, signed in
 * with a numbered slip and signed out when they leave.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, DoorOpen, LogOut, Printer, UserPlus } from "lucide-react";

import { api } from "../lib/api";
import { VISIT_PURPOSES, type DocumentEnvelope, type StudentRow, type Visitor, type VisitorBook } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { PrintPreview, VisitorBookSheet, VisitorSlipSheet } from "../components/PrintDocument";
import { LearnerPicker } from "../components/LearnerPicker";
import { Badge, Button, Card, EmptyState, Loading, Modal, SelectInput, TextInput, formatDate } from "../components/ui";

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export function clock(t: string | null): string {
  if (!t) return "—";
  return new Date(t).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

const purpose = (id: string) => VISIT_PURPOSES.find((p) => p.id === id)?.label ?? id;

type Printing = { kind: "slip"; envelope: DocumentEnvelope<Visitor> } | { kind: "book"; envelope: DocumentEnvelope<VisitorBook> };

export function VisitorsScreen() {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [day, setDay] = useState(iso(new Date()));
  const [book, setBook] = useState<VisitorBook | null>(null);
  const [adding, setAdding] = useState(false);
  const [printing, setPrinting] = useState<Printing | null>(null);
  const mayKeep = can("issue_pass_outs") || can("manage_learners");

  const load = useCallback(() => {
    api.visitorBook(day, day).then(setBook).catch(reportError);
  }, [day, reportError]);
  useEffect(load, [load]);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview actions={<Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>Back</Button>}>
          {printing.kind === "slip" ? <VisitorSlipSheet envelope={printing.envelope} /> : <VisitorBookSheet envelope={printing.envelope} />}
        </PrintPreview>
      </div>
    );
  }

  const signOut = (v: Visitor) =>
    api
      .signOutVisitor(v.id)
      .then(() => {
        toast("success", `${v.fullName} signed out.`);
        load();
      })
      .catch(reportError);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Visitors</h1>
            <p className="page-description">Everyone who comes through the gate, with a numbered slip they carry and hand back when they leave.</p>
          </div>
          {mayKeep && (
            <Button variant="primary" icon={<UserPlus size={15} />} onClick={() => setAdding(true)}>
              Sign in a visitor
            </Button>
          )}
        </div>
        {!book ? (
          <Loading />
        ) : (
          <>
            <Card flush title={`On the compound now (${book.inside.length})`} subtitle="Signed in and not yet signed out, whatever day they came.">
              {book.inside.length === 0 ? (
                <EmptyState icon={<DoorOpen size={20} />} title="No visitors inside">Everyone who came has signed out.</EmptyState>
              ) : (
                <VisitorTable rows={book.inside} onSignOut={mayKeep ? signOut : null} onPrint={(v) => api.buildVisitorSlip(v.id).then((envelope) => setPrinting({ kind: "slip", envelope })).catch(reportError)} />
              )}
            </Card>
            <div className="row" style={{ gap: 12, alignItems: "end" }}>
              <div style={{ width: 180 }}>
                <TextInput label="Visitors' book for" type="date" value={day} max={iso(new Date())} onChange={(e) => setDay(e.target.value)} />
              </div>
              <span className="grow" />
              <Button icon={<Printer size={15} />} onClick={() => api.buildVisitorBook(day, day).then((envelope) => setPrinting({ kind: "book", envelope })).catch(reportError)}>
                Print the day
              </Button>
            </div>
            <Card flush title={`${formatDate(day)}: ${book.visitors.length} visitor${book.visitors.length === 1 ? "" : "s"}`}>
              {book.visitors.length === 0 ? (
                <EmptyState title="Nobody signed in that day">Choose another day.</EmptyState>
              ) : (
                <VisitorTable rows={book.visitors} onSignOut={null} onPrint={null} />
              )}
            </Card>
          </>
        )}
      </div>
      {adding && (
        <SignInModal
          onClose={(slip) => {
            setAdding(false);
            load();
            if (slip) setPrinting({ kind: "slip", envelope: slip });
          }}
        />
      )}
    </div>
  );
}

function VisitorTable({ rows, onSignOut, onPrint }: { rows: Visitor[]; onSignOut: ((v: Visitor) => void) | null; onPrint: ((v: Visitor) => void) | null }) {
  return (
    <div className="table-wrap">
      <table className="table table-compact">
        <thead>
          <tr>
            <th>Slip</th>
            <th>Visitor</th>
            <th>Came for</th>
            <th>In</th>
            <th>Out</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((v) => (
            <tr key={v.id}>
              <td className="mono">{v.slip}</td>
              <td>
                <div style={{ fontWeight: 600 }}>{v.fullName}</div>
                <div className="subtle">{[v.phone, v.organisation, v.vehicle].filter(Boolean).join(", ")}</div>
              </td>
              <td>
                {purpose(v.purpose)}
                <div className="subtle">{v.studentName ? `${v.studentName} (${v.className ?? ""})` : v.toSee ?? ""}</div>
              </td>
              <td style={{ whiteSpace: "nowrap" }}>
                {clock(v.timeIn)}
                {iso(new Date(v.timeIn)) !== iso(new Date()) && <div className="subtle">{formatDate(v.timeIn)}</div>}
              </td>
              <td>{v.timeOut ? clock(v.timeOut) : <Badge tone="warning">Inside</Badge>}</td>
              <td className="right" style={{ whiteSpace: "nowrap" }}>
                {onPrint && <Button size="sm" variant="ghost" icon={<Printer size={14} />} title="Print slip" onClick={() => onPrint(v)} />}
                {onSignOut && !v.timeOut && (
                  <Button size="sm" icon={<LogOut size={14} />} onClick={() => onSignOut(v)}>
                    Signed out
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SignInModal({ onClose }: { onClose: (slip: DocumentEnvelope<Visitor> | null) => void }) {
  const reportError = useStore((state) => state.reportError);
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [idNumber, setIdNumber] = useState("");
  const [organisation, setOrganisation] = useState("");
  const [why, setWhy] = useState("parent");
  const [toSee, setToSee] = useState("");
  const [learner, setLearner] = useState<StudentRow | null>(null);
  const [vehicle, setVehicle] = useState("");
  const [busy, setBusy] = useState(false);

  const save = () => {
    setBusy(true);
    api
      .signInVisitor({
        fullName: name,
        phone: phone || null,
        idNumber: idNumber || null,
        organisation: organisation || null,
        purpose: why,
        toSee: toSee || null,
        studentId: why === "parent" ? learner?.id ?? null : null,
        vehicle: vehicle || null,
        note: null,
      })
      .then(onClose)
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      title="Sign in a visitor"
      description="A numbered slip prints for the visitor to carry and hand back at the gate."
      onClose={() => onClose(null)}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(null)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} disabled={name.trim().length < 3 || (why === "parent" && !learner)} onClick={save}>
            Sign in and print slip
          </Button>
        </>
      }
    >
      <div className="stack">
        <div className="grid-form" style={{ gridTemplateColumns: "1.4fr 1fr" }}>
          <TextInput label="Visitor's name" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          <TextInput label="Phone" value={phone} onChange={(e) => setPhone(e.target.value)} />
        </div>
        <SelectInput label="Came for" value={why} onChange={(e) => setWhy(e.target.value)}>
          {VISIT_PURPOSES.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </SelectInput>
        {why === "parent" ? (
          <LearnerPicker label="Learner they came to see" value={learner} onChange={setLearner} />
        ) : (
          <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr" }}>
            <TextInput label="To see" placeholder="e.g. Headteacher, Bursar" value={toSee} onChange={(e) => setToSee(e.target.value)} />
            <TextInput label="From (organisation)" placeholder="optional" value={organisation} onChange={(e) => setOrganisation(e.target.value)} />
          </div>
        )}
        <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr" }}>
          <TextInput label="ID number" placeholder="optional" value={idNumber} onChange={(e) => setIdNumber(e.target.value)} />
          <TextInput label="Vehicle" placeholder="optional, e.g. UAB 123X" value={vehicle} onChange={(e) => setVehicle(e.target.value)} />
        </div>
      </div>
    </Modal>
  );
}
