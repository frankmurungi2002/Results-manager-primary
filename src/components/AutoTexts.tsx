/**
 * Automatic texts to parents: the switches (Settings → SMS), and the one
 * "look before you send" window used by results texts and fee reminders.
 */

import { useEffect, useState } from "react";
import { MessageSquare, Send } from "lucide-react";

import { api } from "../lib/api";
import { ugx, type AutoSmsSettings, type TextPlan } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Button, Card, Loading, Modal, SelectInput, Switch, TextInput } from "./ui";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

export function AutoSmsCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const offlineOnly = useStore((state) => state.offlineOnly);
  const [settings, setSettings] = useState<AutoSmsSettings | null>(null);
  const [above, setAbove] = useState("0");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getAutoSms()
      .then((s) => {
        setSettings(s);
        setAbove(String(s.feeReminderAbove));
      })
      .catch(reportError);
  }, [reportError]);

  if (!settings) return <Loading />;

  const row = (key: keyof AutoSmsSettings, title: string, body: string) => (
    <label className="auto-row">
      <Switch
        checked={settings[key] as boolean}
        disabled={offlineOnly}
        label={title}
        onChange={(on) => setSettings({ ...settings, [key]: on })}
      />
      <span>
        <strong>{title}</strong>
        <span className="subtle">{body}</span>
      </span>
    </label>
  );

  return (
    <Card
      title="Automatic texts to parents"
      subtitle="Each is off until you switch it on. A parent with several children gets one text naming them all."
      footer={
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <Button
            variant="primary"
            loading={busy}
            disabled={offlineOnly}
            onClick={() => {
              setBusy(true);
              api
                .saveAutoSms({ ...settings, feeReminderAbove: Number(above.replace(/\D/g, "")) || 0 })
                .then(() => toast("success", "Automatic texts saved."))
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save
          </Button>
        </div>
      }
    >
      {offlineOnly && <Alert tone="info">This school uses Termbook offline only, so no text is sent.</Alert>}
      <div className="stack">
        {row("absence", "Absence", "When a learner is newly marked absent on the register, text their parent the same morning.")}
        {row("results", "Results", "Adds “Text results to parents” to Reports: each child's aggregate, division and position.")}
        {row("feeReminders", "Weekly fee reminders", "Once a week, to parents whose balance is above the amount below.")}
        {settings.feeReminders && (
          <div className="grid-form auto-indent">
            <SelectInput label="Day" value={settings.feeReminderDay} onChange={(e) => setSettings({ ...settings, feeReminderDay: Number(e.target.value) })}>
              {DAYS.map((d, i) => (
                <option key={d} value={i + 1}>
                  {d}
                </option>
              ))}
            </SelectInput>
            <TextInput label="Time" type="time" value={settings.feeReminderTime} onChange={(e) => setSettings({ ...settings, feeReminderTime: e.target.value })} />
            <TextInput
              label="Only balances above (UGX)"
              value={above}
              onChange={(e) => setAbove(e.target.value)}
              hint={ugx(Number(above.replace(/\D/g, "")) || 0)}
            />
          </div>
        )}
        {row("promiseReminders", "Promise reminders", "The morning a parent's promise to pay falls due, remind them.")}
        {row("notReported", "Not yet reported", "In the first two weeks of term, text parents of learners who have not come to school at all.")}
        {row("marksReminders", "Marks reminders to teachers", "A day or two before a marks deadline, and on the morning itself, text each teacher who still has marks to enter, naming what is left. Set the deadline in Marks entry.")}
      </div>
      <p className="field-hint" style={{ marginTop: "var(--space-3)" }}>
        Termbook sends these while it is open on the office computer. Texts waiting more than a day are not sent late.
      </p>
    </Card>
  );
}

/** Shows exactly who will be texted, and what it costs, before anything is sent. */
export function TextPlanModal({
  title,
  load,
  send,
  onClose,
}: {
  title: string;
  load: () => Promise<TextPlan>;
  send: () => Promise<number>;
  onClose: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [plan, setPlan] = useState<TextPlan | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    load().then(setPlan).catch((error) => {
      reportError(error);
      onClose();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Modal
      open
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<Send size={15} />}
            loading={busy}
            disabled={!plan || plan.recipients === 0}
            onClick={() => {
              setBusy(true);
              send()
                .then((n) => {
                  toast("success", `${n} text(s) are going out.`);
                  onClose();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            {plan ? `Send ${plan.recipients} text(s)` : "Send"}
          </Button>
        </>
      }
    >
      {!plan ? (
        <Loading label="Working out who to text" />
      ) : (
        <div className="stack">
          <div className="grid grid-stats">
            <div className="plan-figure">
              <strong>{plan.recipients}</strong>
              <span>parents to text</span>
            </div>
            <div className="plan-figure">
              <strong>{plan.learners}</strong>
              <span>learners</span>
            </div>
            <div className="plan-figure">
              <strong>{plan.smsUnits}</strong>
              <span>SMS charged</span>
            </div>
          </div>
          {plan.withoutPhone > 0 && <Alert tone="warning">{plan.withoutPhone} learner(s) have no usable parent's number and will not be texted.</Alert>}
          {plan.skippedRecent > 0 && <Alert tone="info">{plan.skippedRecent} parent(s) were already texted and are left out.</Alert>}
          {plan.sample && (
            <div className="po-preview">
              <MessageSquare size={14} />
              <div>{plan.sample}</div>
            </div>
          )}
          {plan.recipients === 0 && <p className="subtle">Nobody to text.</p>}
        </div>
      )}
    </Modal>
  );
}
