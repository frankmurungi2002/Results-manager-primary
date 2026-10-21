/**
 * Staff → Import from Excel: the school's staff list read from its own
 * spreadsheet. Every new member of staff gets an invite code to set their own
 * password, by SMS and on a printable slip.
 */

import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Upload } from "lucide-react";

import { api } from "../lib/api";
import type { DocumentEnvelope, Invite, StaffColumns, StaffImportPlan } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Button, Modal, SelectInput } from "./ui";

const FIELDS: [keyof StaffColumns, string][] = [
  ["fullName", "Name"],
  ["phone", "Phone"],
  ["email", "Email"],
  ["gender", "Sex"],
  ["position", "Position"],
  ["username", "Username"],
  ["nin", "NIN"],
  ["tmis", "TMIS number"],
];

const STATUS: Record<string, { tone: "success" | "warning" | "neutral"; label: string }> = {
  ready: { tone: "success", label: "Ready" },
  exists: { tone: "neutral", label: "Already on staff" },
  blocked: { tone: "warning", label: "Fix first" },
};

export function StaffImportModal({ onClose, onDone }: { onClose: () => void; onDone: (slips: DocumentEnvelope<Invite>[]) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [path, setPath] = useState<string | null>(null);
  const [plan, setPlan] = useState<StaffImportPlan | null>(null);
  const [skipped, setSkipped] = useState<Set<number>>(new Set());
  const [bySms, setBySms] = useState(true);
  const [busy, setBusy] = useState(false);

  const load = (file: string, columns: StaffColumns | null) => {
    setBusy(true);
    api
      .planStaffImport(file, columns)
      .then((p) => {
        setPlan(p);
        setSkipped(new Set());
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  const pick = async () => {
    const file = await openDialog({ multiple: false, directory: false, filters: [{ name: "Staff list", extensions: ["xlsx", "xls", "csv"] }] });
    if (typeof file !== "string") return;
    setPath(file);
    load(file, null);
  };

  const chosen = plan ? plan.rows.filter((r) => r.status === "ready" && !skipped.has(r.row)) : [];

  const run = () => {
    if (!plan || !path) return;
    setBusy(true);
    api
      .importStaff(path, plan.columns, [...skipped], bySms)
      .then((done) => {
        const { invites, created, smsSent } = done.body;
        toast("success", `${created} member(s) of staff added.${smsSent ? ` ${smsSent} invite code(s) sent by SMS.` : ""} Print the slips for the rest.`);
        onDone(invites.map((invite) => ({ ...done, body: invite })));
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      wide
      title="Import staff from Excel"
      description="Open the staff list the school already keeps (Excel or CSV). Nothing is saved until you press Import."
      onClose={onClose}
      footer={
        <>
          {plan && (
            <label className="row" style={{ marginRight: "auto", gap: 8 }}>
              <input type="checkbox" checked={bySms} onChange={(e) => setBySms(e.target.checked)} />
              <span className="subtle">Also send each invite code by SMS</span>
            </label>
          )}
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" icon={<Upload size={15} />} loading={busy} disabled={!plan || chosen.length === 0} onClick={run}>
            Import {chosen.length} member(s) of staff
          </Button>
        </>
      }
    >
      {!plan ? (
        <div className="stack" style={{ alignItems: "flex-start" }}>
          <Alert tone="info">
            Termbook finds the name, phone, sex and position columns by their headings, and makes a username from each
            name. Bursar, secretary, director of studies and headteacher are read from the position column; everyone
            else is a teacher. Nobody is given a password: each person gets an invite code and sets their own.
          </Alert>
          <Button variant="primary" icon={<FileSpreadsheet size={15} />} loading={busy} onClick={() => void pick()}>
            Open staff list
          </Button>
        </div>
      ) : (
        <div className="stack">
          <div className="grid-form" style={{ gridTemplateColumns: "repeat(4, 1fr)" }}>
            {FIELDS.map(([key, label]) => (
              <SelectInput
                key={key}
                label={label}
                value={plan.columns[key] ?? ""}
                onChange={(e) => path && load(path, { ...plan.columns, [key]: e.target.value === "" ? null : Number(e.target.value) })}
              >
                <option value="">—</option>
                {plan.headers.map((h, i) => (
                  <option key={i} value={i}>
                    {h || `Column ${i + 1}`}
                  </option>
                ))}
              </SelectInput>
            ))}
          </div>
          <div className="table-wrap" style={{ maxHeight: 360, overflow: "auto" }}>
            <table className="table table-compact">
              <thead>
                <tr>
                  <th style={{ width: 36 }} />
                  <th>Row</th>
                  <th>Name</th>
                  <th>Username</th>
                  <th>Office</th>
                  <th>Phone</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {plan.rows.map((r) => (
                  <tr key={r.row}>
                    <td>
                      {r.status === "ready" && (
                        <input
                          type="checkbox"
                          aria-label={`Import ${r.fullName}`}
                          checked={!skipped.has(r.row)}
                          onChange={(e) =>
                            setSkipped((s) => {
                              const next = new Set(s);
                              if (e.target.checked) next.delete(r.row);
                              else next.add(r.row);
                              return next;
                            })
                          }
                        />
                      )}
                    </td>
                    <td className="muted">{r.row}</td>
                    <td>
                      {r.fullName}
                      {[...r.problems, ...r.notes].map((n, i) => (
                        <div key={i} className="subtle" style={{ fontSize: 12 }}>
                          {n}
                        </div>
                      ))}
                    </td>
                    <td className="mono">{r.username}</td>
                    <td>{r.officeLabel}</td>
                    <td className="muted">{r.phone ?? "—"}</td>
                    <td>
                      <Badge tone={STATUS[r.status]?.tone ?? "neutral"}>{STATUS[r.status]?.label ?? r.status}</Badge>
                    </td>
                  </tr>
                ))}
                {plan.rows.length === 0 && (
                  <tr>
                    <td colSpan={7} className="subtle">
                      No staff names were found. Choose the Name column above.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Modal>
  );
}
