/**
 * FR-G22 — pass-outs. A learner leaving school during the day: who, why,
 * where to, with whom and for how long. The guardian is texted when the
 * learner leaves and again when they are back (FR-G8), and a learner still
 * out after their expected time is flagged here and on the dashboard.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  DoorOpen,
  MessageSquare,
  Printer,
  RefreshCw,
  Search,
  Undo2,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  DocumentEnvelope,
  PassOutReason,
  PassOutRow,
  PassOutSlip,
  StudentRow,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Modal,
  StatTile,
  Switch,
  TableSkeleton,
  TextInput,
  cx,
} from "../components/ui";
import { PassOutSlipSheet, PrintPreview } from "../components/PrintDocument";

const REASONS: { value: PassOutReason; label: string }[] = [
  { value: "sick", label: "Sick" },
  { value: "appointment", label: "Appointment" },
  { value: "family", label: "Family matter" },
  { value: "permission", label: "Permission" },
  { value: "other", label: "Other" },
];

const AWAY: { minutes: number | null; label: string }[] = [
  { minutes: 30, label: "30 min" },
  { minutes: 60, label: "1 hr" },
  { minutes: 120, label: "2 hrs" },
  { minutes: 180, label: "3 hrs" },
  { minutes: null, label: "Rest of the day" },
];

function clock(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

function durationWords(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  const h = hours === 0 ? "" : hours === 1 ? "1 hr" : `${hours} hrs`;
  if (hours === 0) return `${mins} min`;
  return mins === 0 ? h : `${h} ${mins} min`;
}

/** "in 40 min" / "20 min late". */
function relativeBack(iso: string, now: number): string {
  const diff = Math.round((new Date(iso).getTime() - now) / 60000);
  return diff >= 0 ? `in ${durationWords(Math.max(diff, 1))}` : `${durationWords(-diff)} late`;
}

export function PassOutsScreen() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);
  const navigate = useStore((state) => state.navigate);

  const [rows, setRows] = useState<PassOutRow[] | null>(null);
  const [smsReady, setSmsReady] = useState<boolean | null>(null);
  const [creating, setCreating] = useState(false);
  const [slip, setSlip] = useState<DocumentEnvelope<PassOutSlip> | null>(null);
  const [now, setNow] = useState(Date.now());

  const load = useCallback(() => {
    api
      .listPassOuts()
      .then(setRows)
      .catch(reportError);
    setNow(Date.now());
  }, [reportError]);

  useEffect(() => {
    load();
    // Keep "late" times and SMS delivery states current while the screen is open.
    const timer = window.setInterval(load, 30_000);
    return () => window.clearInterval(timer);
  }, [load]);

  useEffect(() => {
    if (!isAdmin) return;
    api
      .getSmsSettings()
      .then((settings) => setSmsReady(settings.provider !== "off"))
      .catch(() => setSmsReady(null));
  }, [isAdmin]);

  const out = rows?.filter((row) => row.status === "out") ?? [];
  const overdue = out.filter((row) => row.overdue);
  const returned = rows?.filter((row) => row.status === "returned") ?? [];

  async function openSlip(id: string) {
    try {
      setSlip(await api.buildPassOutSlip(id));
    } catch (error) {
      reportError(error);
    }
  }

  async function markReturned(row: PassOutRow) {
    try {
      const texted = row.smsStatus !== null;
      await api.markPassOutReturned(row.id, texted);
      toast(
        "success",
        texted
          ? `${row.studentName} is back. The guardian is being told.`
          : `${row.studentName} is back.`,
      );
      load();
      // The text goes out in the background; show its result shortly.
      window.setTimeout(load, 5000);
    } catch (error) {
      reportError(error);
    }
  }

  if (slip) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setSlip(null)}>
              Back
            </Button>
          }
        >
          <PassOutSlipSheet envelope={slip} />
        </PrintPreview>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Pass-outs</h1>
            <p className="page-description">
              Learners who leave school during the day. The guardian gets an SMS
              saying where their child is going and for how long, and another
              when they are back.
            </p>
          </div>
          <div className="row">
            <Button variant="ghost" icon={<RefreshCw size={15} />} onClick={load}>
              Refresh
            </Button>
            <Button
              variant="primary"
              icon={<DoorOpen size={15} />}
              onClick={() => setCreating(true)}
            >
              New pass-out
            </Button>
          </div>
        </div>

        {isAdmin && smsReady === false && (
          <Alert tone="info" title="SMS is not set up yet">
            Pass-outs still work and print, and every text waits safely in the
            outbox. They go out as soon as you connect an SMS provider.{" "}
            <button className="link" onClick={() => navigate("settings", { tab: "sms" })}>
              Set up SMS
            </button>
          </Alert>
        )}

        {overdue.length > 0 && (
          <Alert tone="warning" title={`${overdue.length} learner${overdue.length === 1 ? " is" : "s are"} late back`}>
            {overdue
              .map((row) => `${row.studentName} (${row.className}), due at ${clock(row.expectedBack)}`)
              .join("; ")}
            . Call the guardian, or mark them returned when they arrive.
          </Alert>
        )}

        <div className="grid grid-stats">
          <StatTile label="Out of school now" value={out.length} />
          <StatTile label="Late back" value={overdue.length} />
          <StatTile label="Returned today" value={returned.length} />
          <StatTile label="Issued today" value={rows?.length ?? 0} />
        </div>

        <Card title="Today" subtitle="Everyone out now, and every pass-out issued today" flush>
          {rows === null ? (
            <TableSkeleton rows={4} />
          ) : rows.length === 0 ? (
            <EmptyState icon={<DoorOpen size={18} />} title="Nobody has left school today">
              When a learner leaves early, record it here to print a gate slip
              and text their guardian.
            </EmptyState>
          ) : (
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th style={{ width: 90 }}>No.</th>
                    <th>Learner</th>
                    <th>Reason and destination</th>
                    <th>With</th>
                    <th style={{ width: 70 }}>Out</th>
                    <th style={{ width: 150 }}>Back</th>
                    <th style={{ width: 110 }}>Guardian SMS</th>
                    <th style={{ width: 150 }} />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id}>
                      <td className="mono muted">PO-{String(row.number).padStart(4, "0")}</td>
                      <td>
                        <div style={{ fontWeight: 600 }}>{row.studentName}</div>
                        <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                          {row.className}
                        </div>
                      </td>
                      <td>
                        <div>
                          {REASONS.find((r) => r.value === row.reasonKind)?.label}
                          {row.reason ? ` — ${row.reason}` : ""}
                        </div>
                        {row.destination && (
                          <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                            To {row.destination}
                          </div>
                        )}
                      </td>
                      <td>
                        {row.pickedUpBy ?? "—"}
                        {row.pickedUpRelationship && (
                          <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                            {row.pickedUpRelationship}
                          </div>
                        )}
                      </td>
                      <td className="mono">{clock(row.timeOut)}</td>
                      <td>
                        {row.status === "returned" ? (
                          <Badge tone="success">Back {clock(row.returnedAt)}</Badge>
                        ) : row.expectedBack === null ? (
                          <Badge tone="neutral">Not today</Badge>
                        ) : row.overdue ? (
                          <Badge tone="danger">{relativeBack(row.expectedBack, now)}</Badge>
                        ) : (
                          <div>
                            <span className="mono">{clock(row.expectedBack)}</span>{" "}
                            <span className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                              {relativeBack(row.expectedBack, now)}
                            </span>
                          </div>
                        )}
                      </td>
                      <td>
                        <SmsBadge status={row.smsStatus} error={row.smsError} />
                      </td>
                      <td>
                        <div className="row" style={{ gap: 2, justifyContent: "flex-end" }}>
                          <Button
                            size="sm"
                            variant="ghost"
                            icon={<Printer size={14} />}
                            title="Print the slip"
                            onClick={() => void openSlip(row.id)}
                          />
                          {row.status === "out" && (
                            <Button
                              size="sm"
                              variant="secondary"
                              icon={<Undo2 size={14} />}
                              onClick={() => void markReturned(row)}
                            >
                              Back
                            </Button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>
      </div>

      {creating && (
        <NewPassOutModal
          onClose={() => setCreating(false)}
          onCreated={(row) => {
            setCreating(false);
            load();
            window.setTimeout(load, 5000);
            void openSlip(row.id);
          }}
        />
      )}
    </div>
  );
}

function SmsBadge({ status, error }: { status: string | null; error: string | null }) {
  if (status === null) return <span className="subtle">Not texted</span>;
  if (status === "sent") return <Badge tone="success">Sent</Badge>;
  if (status === "failed")
    return (
      <span title={error ?? ""}>
        <Badge tone="danger">Failed</Badge>
      </span>
    );
  return (
    <span title={error ?? "Waiting for an internet connection"}>
      <Badge tone="warning">Waiting</Badge>
    </span>
  );
}

// ---------------------------------------------------------------------------
// New pass-out
// ---------------------------------------------------------------------------

function NewPassOutModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (row: PassOutRow) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const institutionName = useStore((state) => state.institution?.name ?? "School");

  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<StudentRow[]>([]);
  const [learner, setLearner] = useState<StudentRow | null>(null);
  const [reasonKind, setReasonKind] = useState<PassOutReason>("sick");
  const [reason, setReason] = useState("");
  const [destination, setDestination] = useState("");
  const [pickedUpBy, setPickedUpBy] = useState("");
  const [relationship, setRelationship] = useState("");
  const [pickerPhone, setPickerPhone] = useState("");
  const [awayMinutes, setAwayMinutes] = useState<number | null>(60);
  const [backBy, setBackBy] = useState("");
  const [notify, setNotify] = useState(true);
  const [guardianPhone, setGuardianPhone] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (learner || query.trim().length < 2) {
      setMatches([]);
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .searchStudents(query.trim())
        .then((found) => setMatches(found.filter((s) => s.enrollmentStatus === "active")))
        .catch(reportError);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [query, learner, reportError]);

  function pick(student: StudentRow) {
    setLearner(student);
    setPickedUpBy(student.guardianName ?? "");
    setRelationship(student.guardianRelationship ?? "");
    setPickerPhone(student.guardianPhone ?? "");
    setGuardianPhone(student.guardianPhone ?? "");
  }

  /** A custom "back by" time overrides the preset. */
  const minutes = useMemo(() => {
    if (!backBy) return awayMinutes;
    const [h, m] = backBy.split(":").map(Number);
    const target = new Date();
    target.setHours(h ?? 0, m ?? 0, 0, 0);
    const diff = Math.round((target.getTime() - Date.now()) / 60000);
    return diff > 0 ? diff : null;
  }, [awayMinutes, backBy]);

  // The same wording the backend sends (passouts.rs `leaving_message`), for
  // the preview. The signature is the school name unless set under SMS.
  const preview = useMemo(() => {
    if (!learner) return "";
    const nowText = new Date().toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
    let text = `${institutionName}: ${learner.fullName} left school at ${nowText}`;
    if (pickedUpBy.trim()) {
      text += ` with ${pickedUpBy.trim()}`;
      if (relationship.trim()) text += ` (${relationship.trim()})`;
    }
    text += `. Reason: ${REASONS.find((r) => r.value === reasonKind)?.label}`;
    if (reason.trim()) text += ` - ${reason.trim()}`;
    text += ".";
    if (destination.trim()) text += ` Going to: ${destination.trim()}.`;
    if (minutes) {
      const back = new Date(Date.now() + minutes * 60000).toLocaleTimeString("en-GB", {
        hour: "2-digit",
        minute: "2-digit",
      });
      text += ` Away for about ${durationWords(minutes)}, expected back at ${back}.`;
    } else {
      text += " Not returning today.";
    }
    return text;
  }, [learner, institutionName, pickedUpBy, relationship, reasonKind, reason, destination, minutes]);

  const smsParts = preview.length <= 160 ? 1 : Math.ceil(preview.length / 153);

  async function submit() {
    if (!learner) return;
    setBusy(true);
    try {
      const row = await api.createPassOut({
        studentId: learner.id,
        reasonKind,
        reason: reason.trim() || null,
        destination: destination.trim() || null,
        pickedUpBy: pickedUpBy.trim() || null,
        pickedUpRelationship: relationship.trim() || null,
        pickedUpPhone: pickerPhone.trim() || null,
        awayMinutes: minutes,
        notifyGuardian: notify,
        guardianPhone: guardianPhone.trim() || null,
      });
      onCreated(row);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open
      wide
      title="New pass-out"
      description="Prints a slip for the gate and texts the guardian."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<DoorOpen size={15} />}
            loading={busy}
            disabled={!learner}
            onClick={() => void submit()}
          >
            Issue and print slip
          </Button>
        </>
      }
    >
      <div className="stack">
        {learner ? (
          <div className="po-picked">
            <div>
              <div style={{ fontWeight: 600 }}>{learner.fullName}</div>
              <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
                {learner.className} • {learner.regNumber}
                {learner.guardianName ? ` • Guardian: ${learner.guardianName}` : ""}
              </div>
            </div>
            <Button size="sm" variant="ghost" onClick={() => setLearner(null)}>
              Change
            </Button>
          </div>
        ) : (
          <div className="field">
            <span className="field-label">Learner</span>
            <div className="input-icon">
              <Search size={15} />
              <input
                className="input"
                autoFocus
                placeholder="Type a name or registration number"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            {matches.length > 0 && (
              <div className="po-matches">
                {matches.slice(0, 6).map((student) => (
                  <button key={student.id} className="po-match" onClick={() => pick(student)}>
                    <span style={{ fontWeight: 600 }}>{student.fullName}</span>
                    <span className="subtle">
                      {student.className} • {student.regNumber}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        <div className="field">
          <span className="field-label">Reason</span>
          <div className="row" style={{ flexWrap: "wrap" }}>
            {REASONS.map((entry) => (
              <button
                key={entry.value}
                className={cx("pick", reasonKind === entry.value && "is-selected")}
                style={{ width: "auto" }}
                onClick={() => setReasonKind(entry.value)}
              >
                {entry.label}
              </button>
            ))}
          </div>
        </div>

        <div className="grid-form">
          <TextInput
            label="Details"
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            placeholder="e.g. Fever and headache"
          />
          <TextInput
            label="Going to"
            value={destination}
            onChange={(event) => setDestination(event.target.value)}
            placeholder="e.g. Mulago Hospital, home"
          />
          <TextInput
            label="Picked up by"
            value={pickedUpBy}
            onChange={(event) => setPickedUpBy(event.target.value)}
            placeholder="Name of the adult"
          />
          <TextInput
            label="Relationship"
            value={relationship}
            onChange={(event) => setRelationship(event.target.value)}
            placeholder="e.g. Mother, Uncle"
          />
          <TextInput
            label="Their phone"
            value={pickerPhone}
            onChange={(event) => setPickerPhone(event.target.value)}
          />
        </div>

        <div className="field">
          <span className="field-label">Away for</span>
          <div className="row" style={{ flexWrap: "wrap" }}>
            {AWAY.map((entry) => (
              <button
                key={entry.label}
                className={cx(
                  "pick",
                  !backBy && awayMinutes === entry.minutes && "is-selected",
                )}
                style={{ width: "auto" }}
                onClick={() => {
                  setBackBy("");
                  setAwayMinutes(entry.minutes);
                }}
              >
                {entry.label}
              </button>
            ))}
            <label className="row" style={{ gap: "var(--space-2)" }}>
              <span className="muted" style={{ fontSize: "var(--text-sm)" }}>
                or back by
              </span>
              <input
                type="time"
                className="input"
                style={{ width: 120 }}
                value={backBy}
                onChange={(event) => setBackBy(event.target.value)}
              />
            </label>
          </div>
        </div>

        <div className="po-sms">
          <div className="row-between">
            <label className="row" style={{ gap: "var(--space-3)" }}>
              <Switch
                checked={notify}
                onChange={setNotify}
                label="Text the guardian now, and again when they are back"
              />
              <span style={{ fontWeight: 500 }}>
                Text the guardian now, and again when they are back
              </span>
            </label>
            <input
              className="input"
              style={{ width: 170 }}
              placeholder="Guardian phone"
              value={guardianPhone}
              onChange={(event) => setGuardianPhone(event.target.value)}
              disabled={!notify}
            />
          </div>
          {notify && learner && (
            <div className="po-preview">
              <MessageSquare size={14} />
              <div>
                <div>{preview}</div>
                <div className="subtle" style={{ fontSize: "var(--text-2xs)", marginTop: 4 }}>
                  {preview.length} characters • {smsParts} SMS
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}
