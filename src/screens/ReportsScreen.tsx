/**
 * FR-D1 (partial and final report cards), FR-D2 (class lists),
 * FR-G11 (comment bank) and FR-G12 (bulk print with a skipped-learner summary).
 *
 * Nothing here renders a document itself — everything is handed to the global
 * print pipeline in `PrintDocument.tsx` (FR-D4).
 */

import { useEffect, useState } from "react";
import {
  ArrowLeft,
  FileText,
  IdCard as IdCardIcon,
  List,
  MessageSquareText,
  Printer,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  AcademicYearRow,
  ClassListBody,
  ClassRow,
  ActivityRating,
  CommentBankEntry,
  DocumentEnvelope,
  IdCardBatch,
  Rating,
  ReportCardBatch,
  StudentRow,
  TermRow,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Loading,
  Modal,
  Segmented,
  SelectInput,
  TextArea,
  cx,
} from "../components/ui";
import {
  ClassListSheet,
  IdCardSheets,
  type IdCardFormat,
  PrintPreview,
  ReportCardSheets,
} from "../components/PrintDocument";

type Preview =
  | { kind: "report_cards"; envelope: DocumentEnvelope<ReportCardBatch> }
  | { kind: "class_list"; envelope: DocumentEnvelope<ClassListBody> }
  | { kind: "id_cards"; envelope: DocumentEnvelope<IdCardBatch> }
  | null;

export function ReportsScreen() {
  const reportError = useStore((state) => state.reportError);
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);

  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [classId, setClassId] = useState("");
  const [termId, setTermId] = useState("");
  const [examIds, setExamIds] = useState<string[]>([]);
  const [roster, setRoster] = useState<StudentRow[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [preview, setPreview] = useState<Preview>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [commentsFor, setCommentsFor] = useState<StudentRow | null>(null);
  const [idFor, setIdFor] = useState<"student" | "staff">("student");
  const [idFormat, setIdFormat] = useState<IdCardFormat>("sheet");

  useEffect(() => {
    Promise.all([api.listClasses(), api.listAcademicYears()])
      .then(([loadedClasses, loadedYears]) => {
        setClasses(loadedClasses);
        setYears(loadedYears);
        if (loadedClasses.length > 0) setClassId(loadedClasses[0]!.id);

        const openTerm = loadedYears
          .flatMap((year) => year.terms)
          .find((term) => term.status === "open");
        if (openTerm) setTermId(openTerm.id);
        else {
          const firstTerm = loadedYears[0]?.terms[0];
          if (firstTerm) setTermId(firstTerm.id);
        }
      })
      .catch(reportError)
      .finally(() => setLoading(false));
  }, [reportError]);

  useEffect(() => {
    if (!classId) return;
    api
      .listClassRoster(classId)
      .then((loaded) => {
        setRoster(loaded);
        setSelected([]);
      })
      .catch(reportError);
  }, [classId, reportError]);

  const terms: TermRow[] = years.flatMap((year) => year.terms);
  const isNursery =
    classes.find((entry) => entry.id === classId)?.levelKind === "nursery";
  const term = terms.find((entry) => entry.id === termId);

  async function buildReportCards() {
    setBusy(true);
    try {
      const envelope = await api.buildReportCards({
        classId,
        termId,
        studentIds: selected,
        examIds,
      });
      setPreview({ kind: "report_cards", envelope });
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  async function buildIdCards() {
    setBusy(true);
    try {
      const envelope = await api.buildIdCards(
        idFor === "staff"
          ? { kind: "staff" }
          : { kind: "student", classId, ids: selected },
      );
      setPreview({ kind: "id_cards", envelope });
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  async function buildClassList() {
    setBusy(true);
    try {
      const envelope = await api.buildClassList(classId);
      setPreview({ kind: "class_list", envelope });
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  if (loading) {
    return (
      <div className="page">
        <Loading label="Loading" />
      </div>
    );
  }

  // --- Preview mode: the document fills the screen and owns printing ------

  if (preview) {
    const blocked =
      preview.kind === "report_cards" ? preview.envelope.body.blocked : [];

    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <>
              <Button
                variant="ghost"
                icon={<ArrowLeft size={15} />}
                onClick={() => setPreview(null)}
              >
                Back
              </Button>
              <span className="grow" />
              {preview.kind === "report_cards" && (
                <Badge tone="neutral">
                  {preview.envelope.body.cards.length} report card
                  {preview.envelope.body.cards.length === 1 ? "" : "s"}
                </Badge>
              )}
              {blocked.length > 0 && (
                <Badge tone="warning">{blocked.length} blocked on fees</Badge>
              )}
              {preview.kind === "id_cards" && (
                <>
                  <Segmented
                    value={idFormat}
                    onChange={setIdFormat}
                    options={[
                      { value: "sheet", label: "A4 sheet" },
                      { value: "printer", label: "Card printer" },
                    ]}
                  />
                  <Badge tone="neutral">
                    {preview.envelope.body.cards.length} card
                    {preview.envelope.body.cards.length === 1 ? "" : "s"}
                  </Badge>
                </>
              )}
            </>
          }
        >
          {blocked.length > 0 && (
            <div
              className="no-print"
              style={{ maxWidth: "210mm", margin: "0 auto var(--space-5)" }}
            >
              <Alert tone="warning" title={`${blocked.length} learners were not printed`}>
                {blocked
                  .map((entry) => `${entry.fullName} (${entry.reason})`)
                  .join("; ")}
                . Clear the fees flag on the Learners screen to include them.
              </Alert>
            </div>
          )}

          {preview.kind === "id_cards" ? (
            preview.envelope.body.cards.length === 0 ? (
              <div className="no-print" style={{ maxWidth: "210mm", margin: "0 auto" }}>
                <Card>
                  <EmptyState icon={<IdCardIcon size={18} />} title="Nobody to print">
                    There is nobody in this selection yet.
                  </EmptyState>
                </Card>
              </div>
            ) : (
              <IdCardSheets envelope={preview.envelope} format={idFormat} />
            )
          ) : preview.kind === "report_cards" ? (
            preview.envelope.body.cards.length === 0 ? (
              <div
                className="no-print"
                style={{ maxWidth: "210mm", margin: "0 auto" }}
              >
                <Card>
                  <EmptyState icon={<FileText size={18} />} title="Nothing to print">
                    Every learner in this selection is either blocked on fees or
                    has no marks recorded for this term.
                  </EmptyState>
                </Card>
              </div>
            ) : (
              <ReportCardSheets envelope={preview.envelope} />
            )
          ) : (
            <ClassListSheet envelope={preview.envelope} />
          )}
        </PrintPreview>
      </div>
    );
  }

  // --- Setup mode ---------------------------------------------------------

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Reports &amp; printing</h1>
            <p className="page-description">
              Every document carries your school's name and logo and nothing
              else. Print a whole class in one go, or pick out individual
              learners.
            </p>
          </div>
        </div>

        <Card>
          <div className="grid-form">
            <SelectInput
              label="Class"
              value={classId}
              onChange={(event) => setClassId(event.target.value)}
            >
              {classes.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.name} ({entry.learnerCount} learners)
                </option>
              ))}
            </SelectInput>

            <SelectInput
              label="Term"
              value={termId}
              onChange={(event) => {
                setTermId(event.target.value);
                setExamIds([]);
              }}
            >
              {years.map((year) => (
                <optgroup key={year.id} label={year.label}>
                  {year.terms.map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.name}
                      {entry.status === "open" ? " — open" : ""}
                    </option>
                  ))}
                </optgroup>
              ))}
            </SelectInput>
          </div>

          {term && term.exams.length > 0 && (
            <div className="field" style={{ marginTop: "var(--space-5)" }}>
              <span className="field-label">Which examinations</span>
              <div className="row" style={{ flexWrap: "wrap" }}>
                <button
                  className={cx("pick", examIds.length === 0 && "is-selected")}
                  style={{ width: "auto" }}
                  onClick={() => setExamIds([])}
                >
                  Whole term
                </button>
                {term.exams.map((exam) => (
                  <button
                    key={exam.id}
                    className={cx("pick", examIds.includes(exam.id) && "is-selected")}
                    style={{ width: "auto" }}
                    onClick={() =>
                      setExamIds((current) =>
                        current.includes(exam.id)
                          ? current.filter((id) => id !== exam.id)
                          : [...current, exam.id],
                      )
                    }
                  >
                    {exam.name}
                  </button>
                ))}
              </div>
              <span className="field-hint">
                “Whole term” produces the end-of-term card using every
                examination's weight. Picking one produces a partial report.
              </span>
            </div>
          )}
        </Card>

        <div className="grid grid-2">
          <Card
            title={isNursery ? "Nursery report cards" : "Report cards"}
            subtitle={
              isNursery
                ? "Learning areas, activity ratings and comments for each learner"
                : "Grades, aggregate, position and comments for each learner"
            }
            footer={
              <div className="row-between">
                <span className="field-hint">
                  {selected.length === 0
                    ? `Whole class (${roster.length} learners)`
                    : `${selected.length} selected`}
                </span>
                <Button
                  variant="primary"
                  icon={<Printer size={15} />}
                  loading={busy}
                  disabled={!classId || !termId || roster.length === 0}
                  onClick={() => void buildReportCards()}
                >
                  Build report cards
                </Button>
              </div>
            }
            flush
          >
            {roster.length === 0 ? (
              <EmptyState icon={<FileText size={18} />} title="No learners in this class" />
            ) : (
              <div className="table-wrap" style={{ maxHeight: 320 }}>
                <table className="table table-compact">
                  <thead>
                    <tr>
                      <th style={{ width: 44 }} className="center">
                        <input
                          type="checkbox"
                          aria-label="Select all"
                          checked={selected.length === roster.length && roster.length > 0}
                          onChange={(event) =>
                            setSelected(
                              event.target.checked ? roster.map((s) => s.id) : [],
                            )
                          }
                        />
                      </th>
                      <th>Learner</th>
                      <th style={{ width: 130 }}>Reg. No.</th>
                      <th style={{ width: isNursery ? 170 : 120 }} />
                    </tr>
                  </thead>
                  <tbody>
                    {roster.map((student) => (
                      <tr key={student.id}>
                        <td className="center">
                          <input
                            type="checkbox"
                            aria-label={`Select ${student.fullName}`}
                            checked={selected.includes(student.id)}
                            onChange={(event) =>
                              setSelected((current) =>
                                event.target.checked
                                  ? [...current, student.id]
                                  : current.filter((id) => id !== student.id),
                              )
                            }
                          />
                        </td>
                        <td>{student.fullName}</td>
                        <td className="mono muted">{student.regNumber}</td>
                        <td>
                          <Button
                            size="sm"
                            variant="ghost"
                            icon={<MessageSquareText size={13} />}
                            onClick={() => setCommentsFor(student)}
                          >
                            {isNursery ? "Comments & ratings" : "Comments"}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card>

          <div className="stack">
            <Card
              title="Class list"
              subtitle="Every learner with guardian contacts, ready to post"
              footer={
                <Button
                  icon={<List size={15} />}
                  loading={busy}
                  disabled={!classId}
                  onClick={() => void buildClassList()}
                >
                  Build class list
                </Button>
              }
            >
              <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
                A numbered roll with registration numbers, sex, guardian name
                and phone, totalled by sex at the foot.
              </p>
            </Card>

            <Card
              title="ID cards"
              subtitle="Photo, name, class or role, number and the year it is valid for"
              footer={
                <div className="row-between">
                  <span className="field-hint">
                    {idFor === "staff"
                      ? "Every active staff member"
                      : selected.length === 0
                        ? `Whole class (${roster.length} learners)`
                        : `${selected.length} selected`}
                  </span>
                  <Button
                    variant="primary"
                    icon={<IdCardIcon size={15} />}
                    loading={busy}
                    disabled={idFor === "student" && (!classId || roster.length === 0)}
                    onClick={() => void buildIdCards()}
                  >
                    Build ID cards
                  </Button>
                </div>
              }
            >
              <div className="stack" style={{ gap: "var(--space-3)" }}>
                {isAdmin && (
                  <div className="row-between">
                    <span className="field-label">For</span>
                    <Segmented
                      value={idFor}
                      onChange={setIdFor}
                      options={[
                        { value: "student", label: "Learners" },
                        { value: "staff", label: "Staff" },
                      ]}
                    />
                  </div>
                )}
                <div className="row-between">
                  <span className="field-label">Print on</span>
                  <Segmented
                    value={idFormat}
                    onChange={setIdFormat}
                    options={[
                      { value: "sheet", label: "A4 sheet (8 per page)" },
                      { value: "printer", label: "Card printer" },
                    ]}
                  />
                </div>
                <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
                  Photos print when Student Photos is on under Settings,
                  Optional features. Add them from the Learners and Staff
                  screens.
                </p>
              </div>
            </Card>
          </div>
        </div>
      </div>

      <CommentsModal
        student={commentsFor}
        termId={termId}
        nursery={isNursery}
        onClose={() => setCommentsFor(null)}
      />
    </div>
  );
}

export const RATING_OPTIONS: { value: Rating; label: string }[] = [
  { value: "excellent", label: "Excellent" },
  { value: "very_good", label: "Very good" },
  { value: "good", label: "Good" },
  { value: "fair", label: "Fair" },
  { value: "needs_help", label: "Needs help" },
];

/**
 * FR-G11 — pick a comment from the bank, then edit it for this learner. For a
 * nursery class this is also where the learning activities are rated.
 */
function CommentsModal({
  student,
  termId,
  nursery,
  onClose,
}: {
  student: StudentRow | null;
  termId: string;
  nursery: boolean;
  onClose: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [classBank, setClassBank] = useState<CommentBankEntry[]>([]);
  const [headBank, setHeadBank] = useState<CommentBankEntry[]>([]);
  const [classComment, setClassComment] = useState("");
  const [headComment, setHeadComment] = useState("");
  const [conductComment, setConductComment] = useState("");
  const [activityNames, setActivityNames] = useState<string[]>([]);
  const [ratings, setRatings] = useState<Record<string, Rating>>({});
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!student) return;
    setLoaded(false);
    setClassComment("");
    setHeadComment("");
    setConductComment("");
    setRatings({});
    Promise.all([
      api.listCommentBank("class_teacher"),
      api.listCommentBank("head_teacher"),
      api.getReportComment(student.id, termId),
    ])
      .then(([classEntries, headEntries, saved]) => {
        setClassBank(classEntries);
        setHeadBank(headEntries);
        // What was already written this term, so reopening never blanks it.
        setClassComment(saved.classTeacherComment ?? "");
        setHeadComment(saved.headTeacherComment ?? "");
        setConductComment(saved.conductComment ?? "");
        setActivityNames(saved.activityNames);
        setRatings(
          Object.fromEntries(
            saved.activityRatings.map((entry) => [entry.activity, entry.rating]),
          ),
        );
        setLoaded(true);
      })
      .catch(reportError);
  }, [student, termId, reportError]);

  if (!student) return null;

  return (
    <Modal
      open
      wide
      title={nursery ? `Report for ${student.fullName}` : `Comments for ${student.fullName}`}
      description="Pick a phrase to start from, then change it however you like. Editing here never changes the shared bank."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!loaded}
            onClick={() => {
              setBusy(true);
              const activityRatings: ActivityRating[] | null = nursery
                ? Object.entries(ratings).map(([activity, rating]) => ({
                    activity,
                    rating,
                  }))
                : null;
              api
                .saveReportComment({
                  studentId: student.id,
                  termId,
                  classTeacherComment: classComment.trim() || null,
                  headTeacherComment: headComment.trim() || null,
                  conductComment: conductComment.trim() || null,
                  activityRatings,
                })
                .then(() => {
                  toast("success", "Comments saved.");
                  onClose();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            {nursery ? "Save" : "Save comments"}
          </Button>
        </>
      }
    >
      {!loaded ? (
        <Loading label="Loading" />
      ) : (
        <div className="stack">
          {nursery && (
            <div className="field">
              <span className="field-label">Learning activities</span>
              <div className="rating-grid">
                {activityNames.map((activity) => (
                  <div key={activity} className="rating-row">
                    <span className="rating-name">{activity}</span>
                    <div className="segmented" role="radiogroup" aria-label={activity}>
                      {RATING_OPTIONS.map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          role="radio"
                          aria-checked={ratings[activity] === option.value}
                          className={cx(ratings[activity] === option.value && "is-active")}
                          onClick={() =>
                            setRatings((current) => {
                              const next = { ...current };
                              if (next[activity] === option.value) delete next[activity];
                              else next[activity] = option.value;
                              return next;
                            })
                          }
                        >
                          {option.label}
                        </button>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
              <span className="field-hint">
                Click a rating again to clear it. Change the list of activities
                under Settings, School.
              </span>
            </div>
          )}
          <CommentPicker
            label={nursery ? "Class Teacher's report" : "Class Teacher's comment"}
            bank={classBank}
            value={classComment}
            onChange={setClassComment}
          />
          {nursery && (
            <CommentPicker
              label="Behaviour and cleanliness"
              bank={classBank.filter((entry) => entry.category === "Conduct")}
              value={conductComment}
              onChange={setConductComment}
            />
          )}
          <CommentPicker
            label="Headteacher's comment"
            bank={headBank}
            value={headComment}
            onChange={setHeadComment}
          />
        </div>
      )}
    </Modal>
  );
}

function CommentPicker({
  label,
  bank,
  value,
  onChange,
}: {
  label: string;
  bank: CommentBankEntry[];
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div
        className="row"
        style={{ flexWrap: "wrap", gap: "var(--space-2)", marginBottom: "var(--space-2)" }}
      >
        {bank.slice(0, 8).map((entry) => (
          <button
            key={entry.id}
            className="badge badge-neutral"
            style={{ height: 24, cursor: "pointer", maxWidth: 280 }}
            title={entry.text}
            onClick={() => onChange(entry.text)}
          >
            <span className="truncate">{entry.text}</span>
          </button>
        ))}
      </div>
      <TextArea value={value} onChange={(event) => onChange(event.target.value)} />
    </div>
  );
}
