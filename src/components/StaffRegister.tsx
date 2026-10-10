/**
 * Staff → Sign-in register: who is in today, entering someone who forgot
 * their PIN, each person's month, and the register's settings.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, KeyRound, PenLine, Printer } from "lucide-react";

import { api } from "../lib/api";
import type { DocumentEnvelope, RegisterRow, StaffMonth, StaffRegisterSettings } from "../lib/types";
import { useStore } from "../state/store";
import { PrintPreview, StaffRegisterSheet } from "./PrintDocument";
import { Alert, Badge, Button, Card, Loading, Modal, Segmented, StatTile, Switch, TextInput } from "./ui";

const todayIso = () => new Date().toLocaleDateString("en-CA");
const thisMonth = () => todayIso().slice(0, 7);

export function StaffRegister() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [settings, setSettings] = useState<StaffRegisterSettings | null>(null);
  const [view, setView] = useState<"day" | "month">("day");
  const [date, setDate] = useState(todayIso());
  const [month, setMonth] = useState(thisMonth());
  const [rows, setRows] = useState<RegisterRow[] | null>(null);
  const [monthData, setMonthData] = useState<StaffMonth | null>(null);
  const [entering, setEntering] = useState<RegisterRow | null>(null);
  const [printing, setPrinting] = useState<DocumentEnvelope<StaffMonth> | null>(null);

  useEffect(() => {
    api.getStaffRegisterSettings().then(setSettings).catch(reportError);
  }, [reportError]);

  const loadDay = useCallback(() => {
    setRows(null);
    api.staffRegisterDay(date).then(setRows).catch(reportError);
  }, [date, reportError]);
  useEffect(() => {
    if (view === "day") loadDay();
  }, [view, loadDay]);
  useEffect(() => {
    if (view === "month") api.staffRegisterMonth(month).then(setMonthData).catch(reportError);
  }, [view, month, reportError]);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview actions={<Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>Back</Button>}>
          <StaffRegisterSheet envelope={printing} />
        </PrintPreview>
      </div>
    );
  }
  if (!settings) return <Loading />;

  const saveSettings = (next: StaffRegisterSettings) =>
    api
      .saveStaffRegisterSettings(next)
      .then(() => {
        setSettings(next);
        toast("success", next.enabled ? "Staff can now sign in on the sign-in screen." : "The sign-in register is off.");
      })
      .catch(reportError);

  const present = rows?.filter((r) => r.signedInAt).length ?? 0;
  const late = rows?.filter((r) => r.late).length ?? 0;

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <Card
        title="Staff sign-in register"
        subtitle="Staff sign in and out at the office computer with a PIN, from the sign-in screen"
        actions={
          <label className="row" style={{ gap: 8 }}>
            <span className="subtle">{settings.enabled ? "On" : "Off"}</span>
            <Switch checked={settings.enabled} label="Staff sign-in register" onChange={(on) => void saveSettings({ ...settings, enabled: on })} />
          </label>
        }
      >
        <div className="row" style={{ gap: "var(--space-3)", alignItems: "flex-end", flexWrap: "wrap" }}>
          <TextInput
            label="Late after"
            type="time"
            value={settings.startTime}
            onChange={(e) => setSettings({ ...settings, startTime: e.target.value })}
            onBlur={() => void saveSettings(settings)}
          />
          <p className="subtle" style={{ margin: 0, maxWidth: 520 }}>
            Each person sets their own 4 to 6 digit PIN under Settings → My account. If someone forgets theirs, clear it
            from the Staff list and record them by hand meanwhile.
          </p>
        </div>
      </Card>

      <div className="row-between">
        <Segmented value={view} onChange={setView} options={[{ value: "day", label: "One day" }, { value: "month", label: "The month" }]} />
        {view === "day" ? (
          <TextInput type="date" value={date} max={todayIso()} onChange={(e) => setDate(e.target.value)} />
        ) : (
          <div className="row" style={{ gap: 8 }}>
            <TextInput type="month" value={month} max={thisMonth()} onChange={(e) => setMonth(e.target.value)} />
            <Button icon={<Printer size={15} />} onClick={() => api.buildStaffRegister(month).then(setPrinting).catch(reportError)}>
              Print
            </Button>
          </div>
        )}
      </div>

      {view === "day" ? (
        !rows ? (
          <Loading />
        ) : (
          <>
            <div className="grid grid-stats">
              <StatTile label="In school" value={present} meta={`of ${rows.length} staff`} />
              <StatTile label="Late" value={late} meta={`after ${settings.startTime}`} />
              <StatTile label="Not signed in" value={rows.length - present} />
            </div>
            <Card flush>
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>In</th>
                    <th>Out</th>
                    <th>How</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr key={r.userId}>
                      <td style={{ fontWeight: 600 }}>{r.fullName}</td>
                      <td>
                        {r.signedInAt ?? <span className="subtle">—</span>} {r.late && <Badge tone="warning">late</Badge>}
                      </td>
                      <td>{r.signedOutAt ?? ""}</td>
                      <td className="subtle">{r.method === "office" ? `Office: ${r.note ?? ""}` : r.method === "pin" ? "PIN" : ""}</td>
                      <td style={{ textAlign: "right" }}>
                        <Button size="sm" variant="ghost" icon={<PenLine size={14} />} onClick={() => setEntering(r)}>
                          Record
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<KeyRound size={14} />}
                          title="Clear their PIN so they can set a new one"
                          onClick={() => {
                            if (!window.confirm(`Clear ${r.fullName}'s PIN? They set a new one under My account.`)) return;
                            api.clearStaffPin(r.userId).then(() => toast("success", "PIN cleared.")).catch(reportError);
                          }}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
          </>
        )
      ) : !monthData ? (
        <Loading />
      ) : monthData.days.length === 0 ? (
        <Alert tone="info">Nobody signed in during {monthData.label}.</Alert>
      ) : (
        <Card flush>
          <div className="table-wrap">
            <table className="table table-compact">
              <thead>
                <tr>
                  <th>Name</th>
                  <th className="num">Days in</th>
                  <th className="num">Late</th>
                  <th className="num">Usually in at</th>
                </tr>
              </thead>
              <tbody>
                {monthData.rows.map((r) => (
                  <tr key={r.userId}>
                    <td style={{ fontWeight: 600 }}>{r.fullName}</td>
                    <td className="num">
                      {r.daysIn} of {monthData.days.length}
                    </td>
                    <td className="num">{r.daysLate || "—"}</td>
                    <td className="num">{r.averageIn ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      )}

      {entering && (
        <OfficeEntryModal
          row={entering}
          date={date}
          onClose={() => setEntering(null)}
          onSaved={() => {
            setEntering(null);
            loadDay();
          }}
        />
      )}
    </div>
  );
}

function OfficeEntryModal({ row, date, onClose, onSaved }: { row: RegisterRow; date: string; onClose: () => void; onSaved: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [timeIn, setTimeIn] = useState(row.signedInAt ?? "07:30");
  const [timeOut, setTimeOut] = useState(row.signedOutAt ?? "");
  const [note, setNote] = useState(row.note ?? "");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      title={`Record ${row.fullName}`}
      description={`For ${date}. It shows in the register as entered by the office, with your reason.`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!note.trim() || !timeIn}
            onClick={() => {
              setBusy(true);
              api
                .recordStaffAttendance({ userId: row.userId, onDate: date, signedInAt: timeIn, signedOutAt: timeOut || null, note: note.trim() })
                .then(onSaved)
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="grid-form">
        <TextInput label="In at" type="time" value={timeIn} onChange={(e) => setTimeIn(e.target.value)} />
        <TextInput label="Out at (optional)" type="time" value={timeOut} onChange={(e) => setTimeOut(e.target.value)} />
        <TextInput className="span-2" label="Why the office is entering it" placeholder="e.g. Forgot PIN; at a workshop" value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
    </Modal>
  );
}
