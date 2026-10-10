/**
 * Term requirements: the list for the term, and who has brought what.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ClipboardCheck, Plus, Printer, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import { ugx, type ClassRow, type DocumentEnvelope, type RequirementSheet } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { PrintPreview, RequirementsSheet } from "../components/PrintDocument";
import { Badge, Button, Card, EmptyState, Loading, Modal, SelectInput, StatTile, TextInput } from "../components/ui";

export function RequirementsScreen() {
  const can = useCan();
  const session = useStore((state) => state.session);
  const reportError = useStore((state) => state.reportError);
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [classId, setClassId] = useState("");
  const [sheet, setSheet] = useState<RequirementSheet | null>(null);
  const [editing, setEditing] = useState(false);
  const [printing, setPrinting] = useState<DocumentEnvelope<RequirementSheet> | null>(null);
  const mayEditList = can("manage_academics") || can("manage_learners");

  useEffect(() => {
    api
      .listClasses()
      .then((all) => {
        const mine = can("view_all_learners") ? all : all.filter((c) => session?.classTeacherOf.includes(c.id));
        setClasses(mine.filter((c) => c.learnerCount > 0));
        const first = mine.find((c) => c.learnerCount > 0);
        if (first) setClassId(first.id);
      })
      .catch(reportError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportError]);

  const load = useCallback(() => {
    if (!classId) return;
    api.requirementsSheet(classId, null).then(setSheet).catch(reportError);
  }, [classId, reportError]);
  useEffect(load, [load]);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview actions={<Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>Back</Button>}>
          <RequirementsSheet envelope={printing} />
        </PrintPreview>
      </div>
    );
  }

  const record = (studentId: string, itemId: string, quantity: number, cash: number) =>
    api.recordRequirement(studentId, itemId, quantity, cash).then(load).catch(reportError);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Requirements</h1>
            <p className="page-description">
              What each learner brings this term (reams, brooms, books), ticked as it arrives. The list also prints on
              report cards.
            </p>
          </div>
          <div className="row" style={{ gap: 8 }}>
            {mayEditList && (
              <Button icon={<Plus size={15} />} onClick={() => setEditing(true)}>
                Set the term&apos;s list
              </Button>
            )}
            <Button icon={<Printer size={15} />} disabled={!classId} onClick={() => api.buildRequirementsList(classId, null).then(setPrinting).catch(reportError)}>
              Print who still owes
            </Button>
          </div>
        </div>

        <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
          {classes.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </SelectInput>

        {!sheet ? (
          <Loading />
        ) : sheet.items.length === 0 ? (
          <EmptyState icon={<ClipboardCheck size={20} />} title="No requirements set for this term">
            {mayEditList ? "Set the term's list: what every learner, or one class, brings." : "The office has not set the list yet."}
          </EmptyState>
        ) : (
          <>
            <div className="grid grid-stats" style={{ marginTop: "var(--space-4)" }}>
              <StatTile label="Brought everything" value={`${sheet.complete} of ${sheet.rows.length}`} />
              <StatTile label="Still to bring" value={sheet.rows.length - sheet.complete} />
            </div>
            <Card flush>
              <div className="table-wrap">
                <table className="table table-compact">
                  <thead>
                    <tr>
                      <th>Learner</th>
                      {sheet.items.map((i) => (
                        <th key={i.id} className="center">
                          {i.name}
                          <div className="subtle" style={{ fontWeight: 400 }}>
                            {i.quantity > 1 ? `${i.quantity} each` : ""}
                            {i.cashValue ? ` or ${ugx(i.cashValue)}` : ""}
                          </div>
                        </th>
                      ))}
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {sheet.rows.map((r) => (
                      <tr key={r.studentId}>
                        <td style={{ fontWeight: 600 }}>{r.fullName}</td>
                        {sheet.items.map((item, idx) => {
                          const e = r.entries[idx]!;
                          return (
                            <td key={item.id} className="center">
                              {item.quantity === 1 ? (
                                <input
                                  type="checkbox"
                                  checked={e.quantity >= 1}
                                  onChange={(ev) => void record(r.studentId, item.id, ev.target.checked ? 1 : 0, e.cashPaid)}
                                />
                              ) : (
                                <input
                                  key={e.quantity}
                                  className="input input-sm"
                                  style={{ width: 56, textAlign: "center" }}
                                  type="number"
                                  min={0}
                                  defaultValue={e.quantity}
                                  onBlur={(ev) => {
                                    const q = Number(ev.target.value) || 0;
                                    if (q !== e.quantity) void record(r.studentId, item.id, q, e.cashPaid);
                                  }}
                                />
                              )}
                              {item.cashValue ? (
                                <button
                                  className="link-button"
                                  title={e.cashPaid ? "Paid cash instead: undo" : `Paid ${ugx(item.cashValue)} instead`}
                                  onClick={() => void record(r.studentId, item.id, e.quantity, e.cashPaid ? 0 : item.cashValue!)}
                                >
                                  {e.cashPaid ? "cash ✓" : "cash"}
                                </button>
                              ) : null}
                            </td>
                          );
                        })}
                        <td>{r.outstanding.length === 0 ? <Badge tone="success">Complete</Badge> : <Badge tone="warning">{r.outstanding.length} to bring</Badge>}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Card>
          </>
        )}
      </div>
      {editing && <ListModal classes={classes} onClose={() => { setEditing(false); load(); }} />}
    </div>
  );
}

function ListModal({ classes, onClose }: { classes: ClassRow[]; onClose: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [items, setItems] = useState<Awaited<ReturnType<typeof api.listRequirementItems>> | null>(null);
  const [name, setName] = useState("");
  const [quantity, setQuantity] = useState("1");
  const [cash, setCash] = useState("");
  const [classId, setClassId] = useState("");
  const load = useCallback(() => {
    api.listRequirementItems(null).then(setItems).catch(reportError);
  }, [reportError]);
  useEffect(load, [load]);

  return (
    <Modal open wide title="This term's requirements" description="What learners bring. Leave the class empty for every class." onClose={onClose}>
      <div className="stack">
        {items?.map((i) => (
          <div key={i.id} className="row-between" style={{ borderBottom: "1px solid var(--border-subtle)", padding: "6px 0" }}>
            <span>
              <strong>{i.quantity > 1 ? `${i.quantity} x ` : ""}{i.name}</strong>{" "}
              <span className="subtle">
                {i.className ?? "every class"}
                {i.cashValue ? `, or ${ugx(i.cashValue)}` : ""}
              </span>
            </span>
            <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => api.retireRequirementItem(i.id).then(load).catch(reportError)} />
          </div>
        ))}
        <div className="grid-form" style={{ gridTemplateColumns: "2fr 1fr 1fr 1.4fr auto", alignItems: "end" }}>
          <TextInput label="Item" placeholder="e.g. Ream of paper" value={name} onChange={(e) => setName(e.target.value)} />
          <TextInput label="Each brings" type="number" min={1} value={quantity} onChange={(e) => setQuantity(e.target.value)} />
          <TextInput label="Or cash (UGX)" placeholder="optional" value={cash} onChange={(e) => setCash(e.target.value)} />
          <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
            <option value="">Every class</option>
            {classes.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </SelectInput>
          <Button
            variant="primary"
            icon={<Plus size={15} />}
            disabled={!name.trim()}
            onClick={() =>
              api
                .saveRequirementItem({
                  id: null,
                  termId: null,
                  classId: classId || null,
                  name: name.trim(),
                  quantity: Number(quantity) || 1,
                  cashValue: cash.trim() ? Number(cash.replace(/\D/g, "")) : null,
                })
                .then(() => {
                  setName("");
                  setCash("");
                  load();
                })
                .catch(reportError)
            }
          >
            Add
          </Button>
        </div>
      </div>
    </Modal>
  );
}
