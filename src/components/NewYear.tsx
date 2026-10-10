/**
 * Start a new school year: name it, set its terms, check who moves up,
 * repeats or leaves, then do it in one step.
 */

import { useEffect, useMemo, useState } from "react";
import { ArrowRight, GraduationCap } from "lucide-react";

import { api } from "../lib/api";
import type { PromotionPlan } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Button, Loading, Modal, TextInput, cx } from "./ui";

type Action = "promote" | "repeat" | "leave";

export function NewYearModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [plan, setPlan] = useState<PromotionPlan | null>(null);
  const [step, setStep] = useState<1 | 2>(1);
  const [label, setLabel] = useState("");
  const [terms, setTerms] = useState([
    { name: "Term 1", startDate: "", endDate: "" },
    { name: "Term 2", startDate: "", endDate: "" },
    { name: "Term 3", startDate: "", endDate: "" },
  ]);
  const [actions, setActions] = useState<Record<string, Action>>({});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getPromotionPlan(40)
      .then((p) => {
        setPlan(p);
        setLabel(p.suggestedLabel);
        const initial: Record<string, Action> = {};
        p.classes.forEach((c) => c.learners.forEach((l) => (initial[l.studentId] = l.action as Action)));
        setActions(initial);
      })
      .catch((e) => {
        reportError(e);
        onClose();
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const counts = useMemo(() => {
    const values = Object.values(actions);
    return {
      promote: values.filter((a) => a === "promote").length,
      repeat: values.filter((a) => a === "repeat").length,
      leave: values.filter((a) => a === "leave").length,
    };
  }, [actions]);

  if (!plan) {
    return (
      <Modal open title="Start a new school year" onClose={onClose}>
        <Loading label="Working out who moves up" />
      </Modal>
    );
  }

  const changed = plan.classes.flatMap((c) =>
    c.learners.filter((l) => actions[l.studentId] !== l.action).map((l) => ({ studentId: l.studentId, action: actions[l.studentId]! })),
  );

  async function start() {
    setBusy(true);
    try {
      const result = await api.startNewYear({
        label: label.trim(),
        terms: terms.filter((t) => t.name.trim()).map((t) => ({ name: t.name.trim(), startDate: t.startDate || null, endDate: t.endDate || null })),
        decisions: changed,
      });
      toast(
        "success",
        `${result.yearLabel} has started: ${result.promoted} moved up, ${result.repeating} repeating, ${result.left} left. ${result.firstTerm} is open.`,
      );
      onDone();
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
      title={step === 1 ? "Start a new school year" : `Who goes where in ${label}`}
      description={
        step === 1
          ? `${plan.fromYear} will be closed. Its marks, reports, attendance and fees stay exactly as they are, and fees owed carry into the new year.`
          : `Everyone moves up unless you change them. ${plan.resultsTerm ? `Averages are from ${plan.resultsTerm}; under ${plan.repeatBelow}% is flagged.` : ""}`
      }
      onClose={onClose}
      footer={
        step === 1 ? (
          <>
            <Button onClick={onClose}>Cancel</Button>
            <Button variant="primary" icon={<ArrowRight size={15} />} disabled={!label.trim() || !terms[0]?.name.trim()} onClick={() => setStep(2)}>
              Next: who moves up
            </Button>
          </>
        ) : (
          <>
            <span className="subtle" style={{ marginRight: "auto" }}>
              {counts.promote} move up • {counts.repeat} repeat • {counts.leave} leave
            </span>
            <Button onClick={() => setStep(1)}>Back</Button>
            <Button variant="primary" icon={<GraduationCap size={15} />} loading={busy} onClick={() => void start()}>
              Start {label}
            </Button>
          </>
        )
      }
    >
      {step === 1 ? (
        <div className="stack">
          <TextInput label="The new year" value={label} onChange={(e) => setLabel(e.target.value)} />
          {terms.map((term, i) => (
            <div key={i} className="grid-form" style={{ gridTemplateColumns: "1fr 1fr 1fr" }}>
              <TextInput
                label={i === 0 ? "Term" : undefined}
                value={term.name}
                placeholder="Leave empty for no term"
                onChange={(e) => setTerms(terms.map((t, j) => (j === i ? { ...t, name: e.target.value } : t)))}
              />
              <TextInput
                label={i === 0 ? "Starts" : undefined}
                type="date"
                value={term.startDate}
                onChange={(e) => setTerms(terms.map((t, j) => (j === i ? { ...t, startDate: e.target.value } : t)))}
              />
              <TextInput
                label={i === 0 ? "Ends" : undefined}
                type="date"
                value={term.endDate}
                onChange={(e) => setTerms(terms.map((t, j) => (j === i ? { ...t, endDate: e.target.value } : t)))}
              />
            </div>
          ))}
          <Alert tone="info">
            The first term opens straight away, with the same examinations as this year. Set its fees first (Fees → Fee
            structure → Copy from another term) so balances are right from the first day.
          </Alert>
        </div>
      ) : (
        <div className="stack promo-list">
          {plan.classes.map((c) => (
            <section key={c.classId}>
              <div className="section-title">
                {c.className} <ArrowRight size={13} /> {c.nextClassName ?? "Leaving school"}
              </div>
              <table className="table table-compact">
                <tbody>
                  {c.learners.map((l) => {
                    const action = actions[l.studentId]!;
                    return (
                      <tr key={l.studentId}>
                        <td>
                          {l.fullName} <span className="subtle">{l.regNumber}</span>
                          {l.considerRepeating && <Badge tone="warning">consider repeating</Badge>}
                        </td>
                        <td className="num subtle">{l.average !== null ? `${l.average.toFixed(1)}%` : ""}</td>
                        <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                          {(c.nextClassId ? (["promote", "repeat", "leave"] as Action[]) : (["leave", "repeat"] as Action[])).map((a) => (
                            <button
                              key={a}
                              className={cx("chip", action === a && "is-active")}
                              style={{ marginLeft: 4 }}
                              onClick={() => setActions({ ...actions, [l.studentId]: a })}
                            >
                              {a === "promote" ? `Up to ${c.nextClassName}` : a === "repeat" ? "Repeat" : c.nextClassId ? "Leaves" : "Leaves (completed)"}
                            </button>
                          ))}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </section>
          ))}
        </div>
      )}
    </Modal>
  );
}
