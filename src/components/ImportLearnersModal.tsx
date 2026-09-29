/**
 * FR-G13 — onboarding import.
 *
 * Three steps, always in this order: choose a file, match its columns to Phantom School Manager's
 * fields, then check every row before anything is written. The backend does
 * the reading and the validating (commands/importer.rs); this screen only
 * shows what it found and lets the School Admin decide.
 */

import { useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Upload } from "lucide-react";

import { ApiError, api } from "../lib/api";
import {
  IMPORT_FIELDS,
  type ClassRow,
  type ImportResult,
  type ImportRow,
  type SpreadsheetPreview,
} from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Button, Modal, SelectInput } from "./ui";

type Step = "file" | "map" | "check" | "done";

const PREVIEW_ROWS = 5;

export function ImportLearnersModal({
  open,
  classes,
  onClose,
  onImported,
}: {
  open: boolean;
  classes: ClassRow[];
  onClose: () => void;
  onImported: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [step, setStep] = useState<Step>("file");
  const [path, setPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<SpreadsheetPreview | null>(null);
  const [mapping, setMapping] = useState<Record<string, number>>({});
  const [defaultClassId, setDefaultClassId] = useState("");
  const [check, setCheck] = useState<ImportResult | null>(null);
  const [skipRows, setSkipRows] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);

  function reset() {
    setStep("file");
    setPath(null);
    setPreview(null);
    setMapping({});
    setDefaultClassId("");
    setCheck(null);
    setSkipRows(new Set());
    setBusy(false);
  }

  function close() {
    if (busy) return;
    reset();
    onClose();
  }

  function load(filePath: string, sheet?: string) {
    setBusy(true);
    api
      .readSpreadsheet(filePath, sheet)
      .then((loaded) => {
        setPath(filePath);
        setPreview(loaded);
        setMapping(loaded.suggestedMapping);
        setCheck(null);
        setSkipRows(new Set());
        setStep("map");
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  }

  function chooseFile() {
    openDialog({
      multiple: false,
      filters: [
        { name: "Spreadsheet", extensions: ["xlsx", "xls", "xlsm", "ods", "csv", "tsv"] },
      ],
    })
      .then((picked) => {
        if (typeof picked === "string") load(picked);
      })
      .catch((error) => {
        if (error instanceof ApiError) reportError(error);
      });
  }

  function request(commit: boolean) {
    return {
      path: path!,
      sheet: preview?.sheet ?? null,
      mapping,
      defaultClassId: defaultClassId || null,
      commit,
      skipRows: [...skipRows],
    };
  }

  function runCheck() {
    setBusy(true);
    api
      .importLearners(request(false))
      .then((result) => {
        setCheck(result);
        setStep("check");
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  }

  function runImport() {
    setBusy(true);
    api
      .importLearners(request(true))
      .then((result) => {
        setCheck(result);
        setStep("done");
        toast("success", `Imported ${result.imported} learner${result.imported === 1 ? "" : "s"}.`);
        onImported();
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  }

  const hasName = mapping.fullName !== undefined;
  const hasClass = mapping.className !== undefined || defaultClassId !== "";
  const toImport = check
    ? check.rows.filter((row) => isImportable(row) && !skipRows.has(row.rowNumber)).length
    : 0;

  const footer = (() => {
    switch (step) {
      case "file":
        return (
          <div className="row">
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
          </div>
        );
      case "map":
        return (
          <div className="row">
            <Button variant="ghost" disabled={busy} onClick={() => setStep("file")}>
              Back
            </Button>
            <Button
              variant="primary"
              loading={busy}
              disabled={!hasName || !hasClass}
              onClick={runCheck}
            >
              Check every row
            </Button>
          </div>
        );
      case "check":
        return (
          <div className="row">
            <Button variant="ghost" disabled={busy} onClick={() => setStep("map")}>
              Back
            </Button>
            <Button
              variant="primary"
              icon={<Upload size={15} />}
              loading={busy}
              disabled={toImport === 0}
              onClick={runImport}
            >
              Import {toImport} learner{toImport === 1 ? "" : "s"}
            </Button>
          </div>
        );
      case "done":
        return (
          <div className="row">
            <Button variant="primary" onClick={close}>
              Done
            </Button>
          </div>
        );
    }
  })();

  return (
    <Modal
      open={open}
      wide
      title="Import learners from a spreadsheet"
      description={STEP_DESCRIPTIONS[step]}
      onClose={close}
      footer={footer}
    >
      {step === "file" && <FileStep busy={busy} onChoose={chooseFile} />}

      {step === "map" && preview && (
        <MapStep
          preview={preview}
          mapping={mapping}
          onMapping={setMapping}
          classes={classes}
          defaultClassId={defaultClassId}
          onDefaultClass={setDefaultClassId}
          onSheet={(sheet) => path && load(path, sheet)}
          hasName={hasName}
          hasClass={hasClass}
        />
      )}

      {(step === "check" || step === "done") && check && (
        <CheckStep
          result={check}
          done={step === "done"}
          skipRows={skipRows}
          onToggleSkip={(rowNumber) => {
            const next = new Set(skipRows);
            if (next.has(rowNumber)) next.delete(rowNumber);
            else next.add(rowNumber);
            setSkipRows(next);
          }}
        />
      )}
    </Modal>
  );
}

const STEP_DESCRIPTIONS: Record<Step, string> = {
  file: "Step 1 of 3 — choose the file the school already has.",
  map: "Step 2 of 3 — tell Phantom School Manager which column holds what.",
  check: "Step 3 of 3 — nothing is saved until you press Import.",
  done: "Finished.",
};

// ---------------------------------------------------------------------------
// Step 1 — the file
// ---------------------------------------------------------------------------

function FileStep({ busy, onChoose }: { busy: boolean; onChoose: () => void }) {
  return (
    <div className="stack">
      <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
        Excel (.xlsx, .xls), OpenDocument (.ods) or CSV. One row per learner,
        with a heading row. Title rows above the headings are fine; Phantom School Manager finds
        the headings itself.
      </p>
      <div className="row">
        <Button variant="primary" icon={<FileSpreadsheet size={15} />} loading={busy} onClick={onChoose}>
          Choose a spreadsheet
        </Button>
      </div>
      <Alert tone="info" title="Nothing changes yet">
        Phantom School Manager reads the file and shows you every row with any problems it finds.
        Learners are only added when you confirm.
      </Alert>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 2 — matching columns
// ---------------------------------------------------------------------------

function MapStep({
  preview,
  mapping,
  onMapping,
  classes,
  defaultClassId,
  onDefaultClass,
  onSheet,
  hasName,
  hasClass,
}: {
  preview: SpreadsheetPreview;
  mapping: Record<string, number>;
  onMapping: (next: Record<string, number>) => void;
  classes: ClassRow[];
  defaultClassId: string;
  onDefaultClass: (id: string) => void;
  onSheet: (sheet: string) => void;
  hasName: boolean;
  hasClass: boolean;
}) {
  const columnLabel = (index: number) => preview.headers[index]?.trim() || `Column ${index + 1}`;

  return (
    <div className="stack">
      <div className="row" style={{ gap: "var(--space-4)", flexWrap: "wrap", alignItems: "flex-end" }}>
        <span style={{ fontSize: "var(--text-sm)" }}>
          <strong>{preview.fileName}</strong>
          <span className="muted">
            {" "}
            — {preview.totalRows} row{preview.totalRows === 1 ? "" : "s"}
          </span>
        </span>
        {preview.sheets.length > 1 && (
          <div style={{ minWidth: 200 }}>
            <SelectInput
              label="Sheet"
              value={preview.sheet}
              onChange={(event) => onSheet(event.target.value)}
            >
              {preview.sheets.map((sheet) => (
                <option key={sheet} value={sheet}>
                  {sheet}
                </option>
              ))}
            </SelectInput>
          </div>
        )}
      </div>

      {preview.truncated && (
        <Alert tone="warning" title="Only the first 5,000 rows are read">
          Split the file by class and import it in parts.
        </Alert>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
          gap: "var(--space-3)",
        }}
      >
        {IMPORT_FIELDS.map((field) => {
          const value = mapping[field.key];
          return (
            <SelectInput
              key={field.key}
              label={field.required ? `${field.label} *` : field.label}
              value={value === undefined ? "" : String(value)}
              onChange={(event) => {
                const next = { ...mapping };
                if (event.target.value === "") delete next[field.key];
                else next[field.key] = Number(event.target.value);
                onMapping(next);
              }}
            >
              <option value="">— not in this file —</option>
              {preview.headers.map((_, index) => (
                <option key={index} value={index}>
                  {columnLabel(index)}
                </option>
              ))}
            </SelectInput>
          );
        })}

        <SelectInput
          label={mapping.className === undefined ? "Put everyone in *" : "When the class is blank"}
          hint={
            mapping.className === undefined
              ? "The file has no class column, so choose one."
              : "Optional. Rows whose class Phantom School Manager cannot match go here."
          }
          value={defaultClassId}
          onChange={(event) => onDefaultClass(event.target.value)}
        >
          <option value="">{mapping.className === undefined ? "Choose a class" : "Leave them out"}</option>
          {classes.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </SelectInput>
      </div>

      {!hasName && <Alert tone="warning">Choose the column that holds the learner's name.</Alert>}
      {hasName && !hasClass && (
        <Alert tone="warning">Choose a class column, or a class to put everyone in.</Alert>
      )}

      <div>
        <div className="field-label" style={{ marginBottom: "var(--space-2)" }}>
          First {Math.min(PREVIEW_ROWS, preview.rows.length)} rows
        </div>
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                {preview.headers.map((_, index) => (
                  <th key={index}>{columnLabel(index)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {preview.rows.slice(0, PREVIEW_ROWS).map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {preview.headers.map((_, index) => (
                    <td key={index} className="muted">
                      {row[index] ?? ""}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 3 — every row, checked
// ---------------------------------------------------------------------------

function CheckStep({
  result,
  done,
  skipRows,
  onToggleSkip,
}: {
  result: ImportResult;
  done: boolean;
  skipRows: Set<number>;
  onToggleSkip: (rowNumber: number) => void;
}) {
  const [filter, setFilter] = useState<"all" | "problems">(result.blocked > 0 ? "problems" : "all");

  const shown = useMemo(
    () =>
      filter === "problems"
        ? result.rows.filter((row) => row.status === "blocked" || row.notes.length > 0)
        : result.rows,
    [filter, result.rows],
  );

  return (
    <div className="stack">
      {done ? (
        <Alert tone="success" title={`${result.imported} learner${result.imported === 1 ? "" : "s"} imported`}>
          {result.blocked > 0
            ? `${result.blocked} row${result.blocked === 1 ? " was" : "s were"} left out because of the problems listed below. Fix them in the file and import again; rows already imported will be refused as duplicates.`
            : "Every row went in. The import is recorded in the audit log."}
        </Alert>
      ) : (
        <div className="row" style={{ gap: "var(--space-2)", flexWrap: "wrap" }}>
          <Badge tone="success">{result.ready} ready</Badge>
          {result.blocked > 0 && <Badge tone="danger">{result.blocked} with problems</Badge>}
          {skipRows.size > 0 && <Badge tone="neutral">{skipRows.size} left out by you</Badge>}
          <span className="grow" />
          <Button size="sm" variant={filter === "problems" ? "secondary" : "ghost"} onClick={() => setFilter("problems")}>
            Problems and notes
          </Button>
          <Button size="sm" variant={filter === "all" ? "secondary" : "ghost"} onClick={() => setFilter("all")}>
            All rows
          </Button>
        </div>
      )}

      {!done && result.blocked > 0 && (
        <Alert tone="warning" title="Rows with problems are left out">
          The ready rows can be imported now. Fix the others in the spreadsheet
          and import the file again later.
        </Alert>
      )}

      {shown.length === 0 ? (
        <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
          No problems found. Every row is ready.
        </p>
      ) : (
        <div className="table-wrap" style={{ maxHeight: "45vh" }}>
          <table className="table table-compact">
            <thead>
              <tr>
                <th style={{ width: 50 }} className="num">
                  Row
                </th>
                <th>Learner</th>
                <th style={{ width: 110 }}>Class</th>
                <th style={{ width: 130 }}>Reg. No.</th>
                <th style={{ width: 120 }}>Status</th>
                {!done && <th style={{ width: 90 }} />}
              </tr>
            </thead>
            <tbody>
              {shown.map((row) => (
                <tr key={row.rowNumber}>
                  <td className="num muted">{row.rowNumber}</td>
                  <td>
                    <div style={{ fontWeight: 500 }}>{row.fullName || "—"}</div>
                    {row.problems.map((problem) => (
                      <div key={problem} style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>
                        {problem}
                      </div>
                    ))}
                    {row.notes.map((note) => (
                      <div key={note} className="subtle" style={{ fontSize: "var(--text-xs)" }}>
                        {note}
                      </div>
                    ))}
                  </td>
                  <td className="muted">{row.className ?? "—"}</td>
                  <td className="mono muted">{row.regNumber ?? "New"}</td>
                  <td>
                    <RowStatus row={row} skipped={skipRows.has(row.rowNumber)} />
                  </td>
                  {!done && (
                    <td style={{ textAlign: "right" }}>
                      {isImportable(row) && (
                        <Button size="sm" variant="ghost" onClick={() => onToggleSkip(row.rowNumber)}>
                          {skipRows.has(row.rowNumber) ? "Include" : "Leave out"}
                        </Button>
                      )}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** A row with no problems. "skipped" only means it was left out on the last check. */
function isImportable(row: ImportRow): boolean {
  return row.status === "ready" || row.status === "skipped";
}

function RowStatus({ row, skipped }: { row: ImportRow; skipped: boolean }) {
  if (row.status === "imported") return <Badge tone="success">Imported</Badge>;
  if (row.status === "blocked") return <Badge tone="danger">Problem</Badge>;
  if (skipped) return <Badge tone="neutral">Left out</Badge>;
  return <Badge tone="success">Ready</Badge>;
}
