/**
 * Marks entry → Excel: download a sheet already filled with the class's
 * learners, type the marks in Excel, upload it back. The upload is checked
 * first and saved through the same path as the grid (FR-C3, SRS 14.7).
 */

import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Download, FileSpreadsheet, Upload } from "lucide-react";

import { api } from "../lib/api";
import type { MarksUpload } from "../lib/types";
import { useStore } from "../state/store";
import { keepExport } from "./SaveExport";
import { Alert, Badge, Button, Modal } from "./ui";

export function MarksExcelModal({
  classId,
  className,
  examId,
  examName,
  classSubjectId,
  subjectName,
  onClose,
  onSaved,
}: {
  classId: string;
  className: string;
  examId: string;
  examName: string;
  classSubjectId: string;
  subjectName: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [busy, setBusy] = useState<string | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [upload, setUpload] = useState<MarksUpload | null>(null);

  const download = async (whole: boolean) => {
    setBusy(whole ? "class" : "subject");
    try {
      const file = await api.downloadMarksTemplate(classId, examId, whole ? null : classSubjectId);
      const where = await keepExport(file);
      if (where) toast("success", `Saved the marks sheet (${file.detail}) to ${where}`);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(null);
    }
  };

  const pick = async () => {
    const file = await openDialog({ multiple: false, directory: false, filters: [{ name: "Excel marks sheet", extensions: ["xlsx", "xls"] }] });
    if (typeof file !== "string") return;
    setBusy("check");
    try {
      setPath(file);
      setUpload(await api.checkMarksUpload(file));
    } catch (error) {
      setUpload(null);
      reportError(error);
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    if (!path) return;
    setBusy("save");
    try {
      const result = await api.applyMarksUpload(path);
      const extra = [
        result.skipped.length ? `${result.skipped.length} could not be saved` : "",
        result.warnings.length ? `${result.warnings.length} look unusual, check them in the grid` : "",
      ].filter(Boolean);
      toast(extra.length ? "warning" : "success", `Saved ${result.saved} mark(s) in ${result.subjects} subject(s).${extra.length ? ` ${extra.join("; ")}.` : ""}`);
      onSaved();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Modal
      open
      wide
      title="Marks in Excel"
      description={`${className} · ${examName}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Close</Button>
          {upload && (
            <Button variant="primary" icon={<Upload size={15} />} loading={busy === "save"} disabled={upload.changed === 0} onClick={() => void save()}>
              {upload.changed === 0 ? "Nothing new to save" : `Save ${upload.changed} mark(s)`}
            </Button>
          )}
        </>
      }
    >
      <div className="stack">
        <div className="stack" style={{ gap: 8 }}>
          <strong>1. Download the sheet</strong>
          <span className="subtle">
            It already has every learner, and any marks entered so far. Type each mark, or A for absent. A box left
            empty keeps what Termbook has.
          </span>
          <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
            <Button icon={<Download size={15} />} loading={busy === "subject"} onClick={() => void download(false)}>
              {subjectName} only
            </Button>
            <Button icon={<Download size={15} />} loading={busy === "class"} onClick={() => void download(true)}>
              Whole class, all my subjects
            </Button>
          </div>
        </div>

        <div className="stack" style={{ gap: 8 }}>
          <strong>2. Upload it when the marks are in</strong>
          <span className="subtle">Termbook checks every mark first and shows anything wrong. Nothing is saved until you press Save.</span>
          <div>
            <Button variant={upload ? "secondary" : "primary"} icon={<FileSpreadsheet size={15} />} loading={busy === "check"} onClick={() => void pick()}>
              {upload ? "Choose another file" : "Upload filled sheet"}
            </Button>
          </div>
        </div>

        {upload && (
          <div className="stack">
            {(upload.classId !== classId || upload.examId !== examId) && (
              <Alert tone="info">
                This sheet is for {upload.className}, {upload.examName}. Its marks go there.
              </Alert>
            )}
            <div className="table-wrap">
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Subject</th>
                    <th className="num">In the file</th>
                    <th className="num">Absent</th>
                    <th className="num">New or changed</th>
                  </tr>
                </thead>
                <tbody>
                  {upload.subjects.map((s) => (
                    <tr key={s.classSubjectId}>
                      <td>
                        {s.subjectName} <span className="subtle">out of {s.maxScore}</span>
                      </td>
                      <td className="num">{s.marks}</td>
                      <td className="num">{s.absent}</td>
                      <td className="num">{s.changed}</td>
                    </tr>
                  ))}
                  {upload.subjects.length === 0 && (
                    <tr>
                      <td colSpan={4} className="subtle">
                        No subject in this file is one you enter marks for.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            {upload.problems.length > 0 ? (
              <Alert tone="warning" title={`${upload.problems.length} thing(s) to check. These are not saved; the rest are.`}>
                <ul style={{ margin: 0, paddingLeft: 18, maxHeight: 220, overflow: "auto" }}>
                  {upload.problems.map((p, i) => (
                    <li key={i}>
                      {p.row ? <Badge tone="neutral">Row {p.row}</Badge> : null} {p.learner ? <strong>{p.learner}</strong> : null}
                      {p.learner && p.subject ? ", " : " "}
                      {p.subject ? <strong>{p.subject}</strong> : null}
                      {p.learner || p.subject ? ": " : ""}
                      {p.message}
                    </li>
                  ))}
                </ul>
              </Alert>
            ) : (
              <Alert tone="success">Every mark in the file is fine.</Alert>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
