/**
 * FR-G10: everything the reader may see, in one Excel workbook.
 */

import { useEffect, useState } from "react";
import { FileSpreadsheet } from "lucide-react";

import { api } from "../lib/api";
import type { ExportSection } from "../lib/types";
import { useStore } from "../state/store";
import { keepExport } from "./SaveExport";
import { Button, Modal } from "./ui";

export function ExportDataButton() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button icon={<FileSpreadsheet size={15} />} onClick={() => setOpen(true)}>
        Export to Excel
      </Button>
      {open && <ExportDataModal onClose={() => setOpen(false)} />}
    </>
  );
}

function ExportDataModal({ onClose }: { onClose: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [sections, setSections] = useState<ExportSection[]>([]);
  const [chosen, setChosen] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .exportSections()
      .then((all) => {
        setSections(all);
        setChosen(all.filter((s) => s.allowed).map((s) => s.id));
      })
      .catch(reportError);
  }, [reportError]);

  const run = () => {
    setBusy(true);
    api
      .exportSchoolData(chosen)
      .then(keepExport)
      .then((where) => {
        if (where) {
          toast("success", `Saved to ${where}`);
          onClose();
        }
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  return (
    <Modal
      open
      title="Export to Excel"
      description="One workbook, a sheet for each. It is yours to keep, to give the accountant, or to take to another system."
      onClose={onClose}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} disabled={chosen.length === 0} onClick={run}>
            Export
          </Button>
        </>
      }
    >
      <div className="stack" style={{ gap: 8 }}>
        {sections.map((s) => (
          <label key={s.id} className="row" style={{ gap: 8, opacity: s.allowed ? 1 : 0.5 }}>
            <input
              type="checkbox"
              disabled={!s.allowed}
              checked={chosen.includes(s.id)}
              onChange={(e) => setChosen((c) => (e.target.checked ? [...c, s.id] : c.filter((x) => x !== s.id)))}
            />
            <span>
              {s.label}
              {!s.allowed && <span className="subtle"> (not for your role)</span>}
            </span>
          </label>
        ))}
      </div>
    </Modal>
  );
}
