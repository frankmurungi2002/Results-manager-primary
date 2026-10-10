/**
 * The Day Book: every school day, kept. Money in, who was in school, who
 * left at the gate, texts sent, and the head's note. Opened on the office
 * computer whenever the owner or head wants to look back.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, BookText, MessageSquareText, NotebookPen, Printer } from "lucide-react";

import { api } from "../lib/api";
import { ugx, type DayBook, type DayRow, type DocumentEnvelope, type OwnerSmsSettings } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { DayBookSheet, PrintPreview } from "../components/PrintDocument";
import {
  Alert,
  Badge,
  Button,
  Card,
  Loading,
  Modal,
  StatTile,
  Switch,
  TextArea,
  TextInput,
  cx,
} from "../components/ui";

const iso = (d: Date) => d.toLocaleDateString("en-CA");

function monthStart(): string {
  const d = new Date();
  return iso(new Date(d.getFullYear(), d.getMonth(), 1));
}

function dayLabel(date: string): string {
  const d = new Date(`${date}T00:00:00`);
  return d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
}

function isQuiet(day: DayRow): boolean {
  return (
    (day.feesCollected ?? 0) === 0 &&
    (day.spent ?? 0) === 0 &&
    day.registers === 0 &&
    day.passOuts === 0 &&
    day.visitors === 0 &&
    day.textsSent === 0 &&
    day.learnersAdmitted === 0 &&
    day.learnersLeft === 0 &&
    day.staffSignedIn === 0 &&
    !day.note
  );
}

export function DayBookScreen() {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const [from, setFrom] = useState(monthStart());
  const [to, setTo] = useState(iso(new Date()));
  const [book, setBook] = useState<DayBook | null>(null);
  const [showQuiet, setShowQuiet] = useState(false);
  const [editing, setEditing] = useState<DayRow | null>(null);
  const [printing, setPrinting] = useState<DocumentEnvelope<DayBook> | null>(null);
  const [smsOpen, setSmsOpen] = useState(false);

  const load = useCallback(() => {
    if (!from || !to || from > to) return;
    api.getDayBook(from, to).then(setBook).catch(reportError);
  }, [from, to, reportError]);
  useEffect(load, [load]);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>
              Back
            </Button>
          }
        >
          <DayBookSheet envelope={printing} />
        </PrintPreview>
      </div>
    );
  }

  const days = book ? book.days.filter((d) => showQuiet || !isQuiet(d)) : [];
  const t = book?.totals;

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Day Book</h1>
            <p className="page-description">
              Every day at school, kept: money received, attendance, pass-outs, texts and your own note. Worked out
              from the records themselves, so it is always right and never needs closing at night.
            </p>
          </div>
          <div className="row" style={{ gap: 8 }}>
            {can("manage_school") && (
              <Button icon={<MessageSquareText size={15} />} onClick={() => setSmsOpen(true)}>
                Text me each evening
              </Button>
            )}
            <Button
              icon={<Printer size={15} />}
              onClick={() => api.buildDayBook(from, to).then(setPrinting).catch(reportError)}
            >
              Print
            </Button>
          </div>
        </div>

        <div className="row" style={{ flexWrap: "wrap", gap: "var(--space-3)", alignItems: "flex-end" }}>
          <TextInput label="From" type="date" value={from} max={to} onChange={(e) => setFrom(e.target.value)} />
          <TextInput label="To" type="date" value={to} min={from} onChange={(e) => setTo(e.target.value)} />
          <label className="row" style={{ gap: 8, paddingBottom: 8 }}>
            <Switch checked={showQuiet} onChange={setShowQuiet} label="Show days when nothing happened" />
            <span className="subtle">Show weekends and holidays</span>
          </label>
        </div>

        {!book || !t ? (
          <Loading label="Opening the Day Book" />
        ) : (
          <>
            <div className="grid grid-stats fees-stats" style={{ marginTop: "var(--space-4)" }}>
              {book.money && (
                <StatTile label="Fees received" value={ugx(t.feesCollected ?? 0)} meta={`${t.receipts ?? 0} receipts${t.spent ? `, ${ugx(t.spent)} spent` : ""}`} />
              )}
              <StatTile
                label="Attendance"
                value={t.attendancePercent === null ? "—" : `${t.attendancePercent}%`}
                meta={`${t.absent} absences in ${t.registers} registers`}
              />
              <StatTile label="Pass-outs" value={t.passOuts} meta={t.visitors ? `${t.visitors} visitors` : undefined} />
              <StatTile label="Texts sent" value={t.textsSent} meta={t.textsFailed ? `${t.textsFailed} failed` : undefined} />
            </div>

            <Card flush>
              <div className="table-wrap" style={{ maxHeight: 560 }}>
                <table className="table table-compact daybook-table">
                  <thead>
                    <tr>
                      <th>Day</th>
                      {book.money && <th className="num">Fees</th>}
                      {book.money && <th className="num">Spent</th>}
                      <th className="num">Attendance</th>
                      <th className="num">Absent</th>
                      <th className="num">Pass-outs</th>
                      <th className="num">Texts</th>
                      <th className="num">Staff in</th>
                      <th>Note</th>
                    </tr>
                  </thead>
                  <tbody>
                    {days.map((day) => (
                      <tr key={day.date} className={cx("clickable", isQuiet(day) && "is-quiet")} onClick={() => setEditing(day)}>
                        <td style={{ whiteSpace: "nowrap", fontWeight: 600 }}>{dayLabel(day.date)}</td>
                        {book.money && (
                          <td className="num">
                            {day.feesCollected ? ugx(day.feesCollected) : "—"}
                            {day.receiptsCancelled ? (
                              <Badge tone="danger">{day.receiptsCancelled} cancelled</Badge>
                            ) : null}
                          </td>
                        )}
                        {book.money && <td className="num">{day.spent ? ugx(day.spent) : "—"}</td>}
                        <td className="num">{day.attendancePercent === null ? "—" : `${day.attendancePercent}%`}</td>
                        <td className="num">{day.registers ? day.absent : "—"}</td>
                        <td className="num">{day.passOuts || "—"}</td>
                        <td className="num">{day.textsSent || "—"}</td>
                        <td className="num">
                          {day.staffSignedIn ? `${day.staffSignedIn}${day.staffLate ? ` (${day.staffLate} late)` : ""}` : "—"}
                        </td>
                        <td className="daybook-note">
                          {day.note ? (
                            <span title={day.noteBy ? `Written by ${day.noteBy}` : undefined}>{day.note}</span>
                          ) : (
                            <span className="subtle">
                              <NotebookPen size={12} /> Add a note
                            </span>
                          )}
                        </td>
                      </tr>
                    ))}
                    {days.length === 0 && (
                      <tr>
                        <td colSpan={8} className="subtle" style={{ textAlign: "center", padding: "var(--space-6)" }}>
                          <BookText size={18} /> Nothing recorded in these dates yet.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </Card>
          </>
        )}
      </div>

      {editing && (
        <NoteModal
          day={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
      {smsOpen && <OwnerSmsModal onClose={() => setSmsOpen(false)} />}
    </div>
  );
}

function NoteModal({ day, onClose, onSaved }: { day: DayRow; onClose: () => void; onSaved: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [note, setNote] = useState(day.note ?? "");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      title={new Date(`${day.date}T00:00:00`).toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}
      description="What happened today that the figures do not show: a visitor, a meeting, a problem."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .saveDayNote(day.date, note)
                .then(onSaved)
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save note
          </Button>
        </>
      }
    >
      <div className="stack">
        <dl className="daybook-facts">
          {day.feesCollected !== null && (
            <div>
              <dt>Fees received</dt>
              <dd>
                {ugx(day.feesCollected)} ({day.receipts} receipts, cash {ugx(day.feesCash ?? 0)})
              </dd>
            </div>
          )}
          {day.spent ? (
            <div>
              <dt>Spent</dt>
              <dd>{ugx(day.spent)} (see Expenses for the vouchers)</dd>
            </div>
          ) : null}
          <div>
            <dt>Registers</dt>
            <dd>
              {day.registers} classes: {day.present} present, {day.late} late, {day.absent} absent
            </dd>
          </div>
          <div>
            <dt>Gate</dt>
            <dd>{day.passOuts} pass-outs</dd>
          </div>
          <div>
            <dt>Learners</dt>
            <dd>
              {day.learnersAdmitted} admitted, {day.learnersLeft} left
            </dd>
          </div>
        </dl>
        <TextArea rows={5} autoFocus value={note} onChange={(e) => setNote(e.target.value)} placeholder="e.g. District inspector visited P5. Water tank repaired." />
      </div>
    </Modal>
  );
}

function OwnerSmsModal({ onClose }: { onClose: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const offlineOnly = useStore((state) => state.offlineOnly);
  const [settings, setSettings] = useState<OwnerSmsSettings | null>(null);
  const [phones, setPhones] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getOwnerSms()
      .then((s) => {
        setSettings(s);
        setPhones(s.phones.join(", "));
      })
      .catch(reportError);
  }, [reportError]);

  return (
    <Modal
      open
      title="Text me the day's summary"
      description="One SMS each evening, only on days something happened. Off unless you switch it on."
      onClose={onClose}
      footer={
        settings && (
          <>
            <Button onClick={onClose}>Cancel</Button>
            <Button
              variant="primary"
              loading={busy}
              onClick={() => {
                setBusy(true);
                api
                  .saveOwnerSms({
                    ...settings,
                    phones: phones.split(",").map((p) => p.trim()).filter(Boolean),
                  })
                  .then(() => {
                    toast("success", settings.enabled ? `The summary will be texted at ${settings.sendAt}.` : "The evening text is off.");
                    onClose();
                  })
                  .catch(reportError)
                  .finally(() => setBusy(false));
              }}
            >
              Save
            </Button>
          </>
        )
      }
    >
      {!settings ? (
        <Loading />
      ) : (
        <div className="stack">
          {offlineOnly && <Alert tone="info">This school uses Termbook offline only, so no text can be sent.</Alert>}
          <label className="row" style={{ gap: "var(--space-3)" }}>
            <Switch
              checked={settings.enabled}
              disabled={offlineOnly}
              onChange={(on) => setSettings({ ...settings, enabled: on })}
              label="Send the evening summary"
            />
            <span>Send the evening summary (one SMS per number per day)</span>
          </label>
          <TextInput label="To (up to three numbers, separated by commas)" value={phones} onChange={(e) => setPhones(e.target.value)} placeholder="0772 123456" />
          <TextInput label="At" type="time" value={settings.sendAt} onChange={(e) => setSettings({ ...settings, sendAt: e.target.value })} />
          <label className="row" style={{ gap: "var(--space-3)" }}>
            <Switch checked={settings.includeMoney} onChange={(on) => setSettings({ ...settings, includeMoney: on })} label="Include fees received" />
            <span>Include the fees received (leave off if the phone is shared)</span>
          </label>
          <Alert tone="info">
            It reads like: “Rainbow N&amp;P, Thu 8 Oct: Fees UGX 1,450,000 (12 receipts; cash 600,000). Attendance 94%, 21
            absent. 3 pass-outs.” Termbook must be open on the office computer at that time.
          </Alert>
        </div>
      )}
    </Modal>
  );
}
