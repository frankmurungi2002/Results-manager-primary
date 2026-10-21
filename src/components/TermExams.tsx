/**
 * Terms & exams → Examinations: rename a term's examinations, change how much
 * each counts in the term mark, add a test, or remove one with no marks.
 */

import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { ExamRow, TermRow } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Button, Modal } from "./ui";

interface Draft {
  id: string | null;
  code: string;
  name: string;
  share: string;
  isFinal: boolean;
}

const toDraft = (exam: ExamRow): Draft => ({
  id: exam.id,
  code: exam.code,
  name: exam.name,
  share: String(Math.round(exam.weight * 1000) / 10),
  isFinal: exam.isFinal,
});

export function TermExamsModal({ term, onClose, onSaved }: { term: TermRow; onClose: () => void; onSaved: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [rows, setRows] = useState<Draft[]>(() => term.exams.map(toDraft));
  const [busy, setBusy] = useState(false);

  const total = rows.reduce((sum, r) => sum + (Number(r.share) || 0), 0);
  const totalOk = total === 0 || Math.abs(total - 100) < 0.5;
  const set = (i: number, patch: Partial<Draft>) => setRows((all) => all.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  const save = () => {
    setBusy(true);
    api
      .saveTermExams(
        term.id,
        rows.map((r) => ({ id: r.id, code: r.code, name: r.name, weight: (Number(r.share) || 0) / 100, isFinal: r.isFinal })),
      )
      .then(() => {
        toast("success", `${term.name} examinations saved. Report cards use the new shares from the next print.`);
        onSaved();
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      wide
      title={`${term.name}: examinations`}
      description="How much each examination counts in the term mark on the report card."
      onClose={onClose}
      footer={
        <>
          <span className="subtle" style={{ marginRight: "auto", color: totalOk ? undefined : "var(--danger)" }}>
            Total {Math.round(total * 10) / 10}%{totalOk ? "" : " (must be 100%, or all 0%)"}
          </span>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} disabled={!totalOk} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div className="stack">
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th style={{ width: 110 }}>Code</th>
                <th>Name</th>
                <th style={{ width: 120 }}>Counts (%)</th>
                <th style={{ width: 120 }}>End of term</th>
                <th style={{ width: 48 }} />
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={r.id ?? `new-${i}`}>
                  <td>
                    <input className="input" aria-label="Code" value={r.code} maxLength={8} onChange={(e) => set(i, { code: e.target.value.toUpperCase() })} />
                  </td>
                  <td>
                    <input className="input" aria-label="Name" value={r.name} onChange={(e) => set(i, { name: e.target.value })} />
                  </td>
                  <td>
                    <input className="input" aria-label="Counts (%)" inputMode="decimal" value={r.share} onChange={(e) => set(i, { share: e.target.value.replace(/[^\d.]/g, "") })} />
                  </td>
                  <td>
                    <label className="row" style={{ gap: 6 }}>
                      <input type="radio" name="final" checked={r.isFinal} onChange={() => setRows((all) => all.map((x, j) => ({ ...x, isFinal: j === i })))} />
                      <span className="subtle">{r.isFinal ? "Yes" : ""}</span>
                    </label>
                  </td>
                  <td>
                    <Button
                      variant="ghost"
                      icon={<Trash2 size={14} />}
                      aria-label={`Remove ${r.name}`}
                      disabled={rows.length === 1}
                      onClick={() => setRows((all) => all.filter((_, j) => j !== i))}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div>
          <Button
            variant="secondary"
            icon={<Plus size={15} />}
            onClick={() => setRows((all) => [...all, { id: null, code: "", name: "", share: "0", isFinal: false }])}
          >
            Add an examination
          </Button>
        </div>
        <Alert tone="info">
          Example: Mid Term 40% and End of Term 60%. Set every share to 0% to use only the end-of-term paper. An
          examination that already has marks can be renamed or set to 0%, but not removed.
        </Alert>
      </div>
    </Modal>
  );
}
