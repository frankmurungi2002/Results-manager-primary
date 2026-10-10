/**
 * Fees → Import SchoolPay statement: open the statement downloaded from the
 * SchoolPay portal, check each line's learner, import as receipts.
 */

import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Upload } from "lucide-react";

import { api } from "../lib/api";
import { ugx, type StatementColumns, type StatementPlan, type StudentRow } from "../lib/types";
import { useStore } from "../state/store";
import { LearnerPicker } from "./LearnerPicker";
import { Alert, Badge, Button, Modal, SelectInput } from "./ui";

const FIELDS: [keyof StatementColumns, string][] = [
  ["reference", "Transaction ID"],
  ["amount", "Amount"],
  ["date", "Date"],
  ["paymentCode", "Payment code"],
  ["regNumber", "Reg. no."],
  ["name", "Learner's name"],
];

const STATUS: Record<string, { tone: "success" | "info" | "warning" | "danger" | "neutral"; label: string }> = {
  ready: { tone: "success", label: "Ready" },
  suggested: { tone: "info", label: "Check the name" },
  duplicate: { tone: "neutral", label: "Already receipted" },
  unmatched: { tone: "warning", label: "Choose the learner" },
  invalid: { tone: "danger", label: "Cannot read" },
};

export function SchoolPayImportModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [path, setPath] = useState<string | null>(null);
  const [plan, setPlan] = useState<StatementPlan | null>(null);
  const [chosen, setChosen] = useState<Record<number, string>>({});
  const [included, setIncluded] = useState<Record<number, boolean>>({});
  const [matching, setMatching] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const load = (file: string, columns: StatementColumns | null) => {
    setBusy(true);
    api
      .readSchoolpayStatement(file, columns)
      .then((p) => {
        setPlan(p);
        const picks: Record<number, string> = {};
        const take: Record<number, boolean> = {};
        p.lines.forEach((l) => {
          if (l.studentId) picks[l.row] = l.studentId;
          take[l.row] = l.status === "ready";
        });
        setChosen(picks);
        setIncluded(take);
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  const pick = async () => {
    const file = await openDialog({ multiple: false, directory: false, filters: [{ name: "Statement", extensions: ["xlsx", "xls", "csv"] }] });
    if (typeof file !== "string") return;
    setPath(file);
    load(file, null);
  };

  const selected = plan ? plan.lines.filter((l) => included[l.row] && chosen[l.row]) : [];
  const total = selected.reduce((sum, l) => sum + (l.amount ?? 0), 0);

  return (
    <Modal
      open
      wide
      title="Import a SchoolPay statement"
      description="Download the statement from the SchoolPay school portal (Excel or CSV), then open it here."
      onClose={onClose}
      footer={
        <>
          <span className="subtle" style={{ marginRight: "auto" }}>
            {plan ? `${selected.length} receipt(s), ${ugx(total)}` : ""}
          </span>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<Upload size={15} />}
            loading={busy}
            disabled={!plan || !path || selected.length === 0}
            onClick={() => {
              if (!plan || !path) return;
              setBusy(true);
              api
                .importSchoolpayStatement(path, plan.columns, selected.map((l) => ({ row: l.row, studentId: chosen[l.row]! })))
                .then((r) => {
                  toast("success", `${r.receipts} receipt(s) recorded, ${ugx(r.amount)}.${r.codesLearned ? ` ${r.codesLearned} SchoolPay code(s) learned.` : ""}`);
                  onDone();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Record {selected.length} receipt(s)
          </Button>
        </>
      }
    >
      {!plan ? (
        <div className="stack" style={{ alignItems: "flex-start" }}>
          <Alert tone="info">
            Each line is matched by the learner&apos;s SchoolPay payment code, then their registration number, then their
            name (which you confirm). A transaction already receipted is skipped, so opening the same statement twice
            never records a payment twice.
          </Alert>
          <Button variant="primary" icon={<FileSpreadsheet size={15} />} loading={busy} onClick={() => void pick()}>
            Open statement
          </Button>
        </div>
      ) : (
        <div className="stack">
          <div className="grid-form" style={{ gridTemplateColumns: "repeat(6, 1fr)" }}>
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
          {plan.lines.some((l) => l.status === "suggested") && (
            <div className="row-between">
              <span className="subtle">Lines matched by name only are left unticked until you check them.</span>
              <Button
                size="sm"
                onClick={() => {
                  const next = { ...included };
                  plan.lines.filter((l) => l.status === "suggested").forEach((l) => (next[l.row] = true));
                  setIncluded(next);
                }}
              >
                Tick every name match
              </Button>
            </div>
          )}
          <div className="table-wrap" style={{ maxHeight: 420 }}>
            <table className="table table-compact">
              <thead>
                <tr>
                  <th />
                  <th>Transaction</th>
                  <th>Date</th>
                  <th className="num">Amount</th>
                  <th>On the statement</th>
                  <th>Learner</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {plan.lines.map((l) => {
                  const status = l.status === "unmatched" && chosen[l.row] ? { tone: "success" as const, label: "Chosen" } : STATUS[l.status]!;
                  const usable = !["duplicate", "invalid"].includes(l.status);
                  return (
                    <tr key={l.row}>
                      <td>
                        <input
                          type="checkbox"
                          disabled={!usable || !chosen[l.row]}
                          checked={!!included[l.row] && !!chosen[l.row]}
                          onChange={(e) => setIncluded({ ...included, [l.row]: e.target.checked })}
                        />
                      </td>
                      <td className="mono">{l.reference}</td>
                      <td>{l.paidOn ?? ""}</td>
                      <td className="num">{l.amount ? ugx(l.amount) : ""}</td>
                      <td>
                        {l.nameOnStatement}
                        {l.paymentCode && <span className="subtle"> {l.paymentCode}</span>}
                      </td>
                      <td>
                        {matching === l.row ? (
                          <LearnerPicker
                            value={null}
                            autoFocus
                            label=""
                            onChange={(s: StudentRow | null) => {
                              if (!s) return;
                              setChosen({ ...chosen, [l.row]: s.id });
                              setIncluded({ ...included, [l.row]: true });
                              setMatching(null);
                              l.studentName = s.fullName;
                              l.className = s.className;
                            }}
                          />
                        ) : (
                          <>
                            {l.studentName && (
                              <>
                                {l.studentName} <span className="subtle">{l.className ?? ""}</span>
                              </>
                            )}
                            {usable && (
                              <button className="link-button" onClick={() => setMatching(l.row)}>
                                {l.studentName ? "Change" : "Choose"}
                              </button>
                            )}
                          </>
                        )}
                      </td>
                      <td title={l.problem ?? undefined}>
                        <Badge tone={status.tone}>{l.problem ?? status.label}</Badge>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </Modal>
  );
}
