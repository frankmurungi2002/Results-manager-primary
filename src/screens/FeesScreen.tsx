/**
 * FR-G5 — fees: take a payment and print its receipt, see who owes, see what
 * came in, set what each class pays.
 *
 * Built around the bursar's day: the first tab is the counter, where a parent
 * stands with money. Everything else is one click away.
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ArrowLeft,
  Ban,
  Banknote,
  CalendarClock,
  Copy,
  FileSpreadsheet,
  FileText,
  HandCoins,
  MessageSquare,
  Plus,
  Printer,
  Receipt,
  Save,
  Settings2,
  Users,
} from "lucide-react";

import { api } from "../lib/api";
import {
  ADJUSTMENT_KINDS,
  PAYMENT_METHODS,
  ugx,
  type AdjustmentKind,
  type Collections,
  type FeeAccountDetail,
  type FeeAccountList,
  type FeeAccountRow,
  type FeeItemKind,
  type FeeSettings,
  type FeeSetup,
  type OptinRow,
  type PaymentMethod,
  type PaymentRow,
  type StudentRow,
} from "../lib/types";
import { useCan, useStore } from "../state/store";
import { LearnerPicker } from "../components/LearnerPicker";
import { TextPlanModal } from "../components/AutoTexts";
import { SchoolPayImportModal } from "../components/SchoolPayImport";
import {
  FeeBalancesSheet,
  FeeCollectionsSheet,
  FeeInvoiceSheets,
  FeeReceiptSheet,
  FeeStatementSheet,
  PrintPreview,
} from "../components/PrintDocument";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Loading,
  Modal,
  SelectInput,
  StatTile,
  Switch,
  TextArea,
  TextInput,
  cx,
} from "../components/ui";

type Tab = "counter" | "balances" | "collections" | "structure" | "settings";

const today = () => new Date().toLocaleDateString("en-CA");

function shortDate(iso: string): string {
  const d = new Date(iso.length === 10 ? `${iso}T00:00:00` : iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** Whole shillings from what was typed: "250,000" and "250000" alike. */
function parseShillings(text: string): number | null {
  const cleaned = text.replace(/[,\s]/g, "");
  if (!/^\d+$/.test(cleaned)) return null;
  return Number(cleaned);
}

function balanceTone(balance: number): "danger" | "success" | "info" {
  return balance > 0 ? "danger" : balance === 0 ? "success" : "info";
}

function balanceText(balance: number): string {
  return balance > 0 ? ugx(balance) : balance === 0 ? "Cleared" : `${ugx(-balance)} credit`;
}

export function FeesScreen() {
  const can = useCan();
  const params = useStore((state) => state.screenParams);
  const [tab, setTab] = useState<Tab>((params.tab as Tab) ?? "counter");
  const [document, setDocument] = useState<ReactNode | null>(null);

  if (document) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setDocument(null)}>
              Back
            </Button>
          }
        >
          {document}
        </PrintPreview>
      </div>
    );
  }

  const tabs: [Tab, string, boolean][] = [
    ["counter", "Take a payment", can("manage_fees")],
    ["balances", "Balances", true],
    ["collections", "Collections", true],
    ["structure", "Fee structure", true],
    ["settings", "Fees settings", can("manage_fees")],
  ];

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Fees</h1>
            <p className="page-description">
              Every payment gets a numbered receipt that cannot be changed or deleted. Balances work themselves out
              from the fee structure, bursaries and payments, and carry from term to term.
            </p>
          </div>
        </div>

        <div className="tabs">
          {tabs
            .filter(([, , allowed]) => allowed)
            .map(([id, label]) => (
              <button key={id} className={cx("tab", tab === id && "is-active")} onClick={() => setTab(id)}>
                {label}
              </button>
            ))}
        </div>

        {tab === "counter" && <Counter onPrint={setDocument} />}
        {tab === "balances" && <Balances onPrint={setDocument} />}
        {tab === "collections" && <CollectionsTab onPrint={setDocument} />}
        {tab === "structure" && <Structure />}
        {tab === "settings" && <SettingsTab />}
      </div>
    </div>
  );
}

type Print = (node: ReactNode) => void;

// ---------------------------------------------------------------------------
// The counter: a parent is standing here with money
// ---------------------------------------------------------------------------

function Counter({ onPrint }: { onPrint: Print }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const offlineOnly = useStore((state) => state.offlineOnly);
  const [learner, setLearner] = useState<StudentRow | null>(null);
  const [account, setAccount] = useState<FeeAccountDetail | null>(null);
  const [settings, setSettings] = useState<FeeSettings | null>(null);
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [paidBy, setPaidBy] = useState("");
  const [payerPhone, setPayerPhone] = useState("");
  const [paidOn, setPaidOn] = useState(today());
  const [note, setNote] = useState("");
  const [textGuardian, setTextGuardian] = useState(true);
  const [busy, setBusy] = useState(false);
  const [last, setLast] = useState<PaymentRow | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    api.getFeeSettings().then(setSettings).catch(reportError);
  }, [reportError]);

  useEffect(() => {
    setAccount(null);
    if (!learner) return;
    setPaidBy(learner.guardianName ?? "");
    setPayerPhone(learner.guardianPhone ?? "");
    api.getFeeAccount(learner.id).then(setAccount).catch(reportError);
  }, [learner, reportError]);

  const methodInfo = PAYMENT_METHODS.find((m) => m.id === method)!;
  const shillings = parseShillings(amount);
  const after = account && shillings ? account.balance - shillings : null;
  const thisTerm = account?.terms[account.terms.length - 1];

  const reset = () => {
    setLearner(null);
    setAmount("");
    setReference("");
    setNote("");
    setMethod("cash");
    setPaidOn(today());
  };

  async function record() {
    if (!learner || !shillings) return;
    setBusy(true);
    try {
      const recorded = await api.recordFeePayment({
        studentId: learner.id,
        amount: shillings,
        method,
        reference: reference.trim() || null,
        paidBy: paidBy.trim() || null,
        payerPhone: payerPhone.trim() || null,
        paidOn,
        note: note.trim() || null,
        textGuardian: textGuardian && !offlineOnly,
      });
      setLast(recorded.payment);
      toast(
        "success",
        `${recorded.payment.receiptLabel}: ${ugx(recorded.payment.amount)} from ${learner.fullName}.` +
          (recorded.guardianTexted ? " The parent is being texted." : "") +
          (recorded.promisesKept ? " A promise to pay was kept." : ""),
      );
      reset();
      const envelope = await api.buildFeeReceipt(recorded.payment.id);
      onPrint(<FeeReceiptSheet envelope={envelope} />);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fees-counter">
      <Card title="Take a payment" subtitle="Find the learner, enter what was paid, print the receipt">
        <div className="stack">
          <LearnerPicker value={learner} onChange={setLearner} autoFocus includeLeft />

          {learner && !account && <Loading label="Loading the account" />}

          {learner && account && (
            <>
              <div className="fees-owes">
                <div>
                  <span>Owes now</span>
                  <strong className={`tone-${balanceTone(account.balance)}`}>{balanceText(account.balance)}</strong>
                </div>
                {thisTerm && (
                  <div>
                    <span>{thisTerm.termName} bill</span>
                    <strong>{ugx(thisTerm.billed)}</strong>
                  </div>
                )}
                {thisTerm && (
                  <div>
                    <span>Paid this term</span>
                    <strong>{ugx(thisTerm.paid)}</strong>
                  </div>
                )}
              </div>
              {account.promises.some((p) => p.status === "open") && (
                <Alert tone="info" title="Promise to pay">
                  {account.promises
                    .filter((p) => p.status === "open")
                    .map((p) => `${ugx(p.amount)} by ${shortDate(p.dueOn)}${p.promisedBy ? ` (${p.promisedBy})` : ""}`)
                    .join("; ")}
                </Alert>
              )}

              <div className="grid-form">
                <TextInput
                  label="Amount received (UGX)"
                  inputMode="numeric"
                  autoFocus
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  hint={shillings ? ugx(shillings) : "Whole shillings, e.g. 250000"}
                  error={amount && !shillings ? "Numbers only" : undefined}
                />
                <TextInput label="Date paid" type="date" max={today()} value={paidOn} onChange={(e) => setPaidOn(e.target.value)} />
                <div className="field" style={{ gridColumn: "1 / -1" }}>
                  <span className="field-label">How it was paid</span>
                  <div className="row" style={{ flexWrap: "wrap" }}>
                    {PAYMENT_METHODS.map((m) => (
                      <button
                        key={m.id}
                        className={cx("pick", method === m.id && "is-selected")}
                        style={{ width: "auto" }}
                        onClick={() => setMethod(m.id)}
                      >
                        {m.label}
                      </button>
                    ))}
                  </div>
                </div>
                <TextInput
                  label={methodInfo.referenceLabel}
                  value={reference}
                  onChange={(e) => setReference(e.target.value)}
                  hint={methodInfo.needsReference ? "Required: one transaction is receipted only once" : undefined}
                />
                <TextInput label="Paid by" value={paidBy} onChange={(e) => setPaidBy(e.target.value)} />
                <TextInput label="Payer's phone" value={payerPhone} onChange={(e) => setPayerPhone(e.target.value)} />
                <TextInput label="Note (optional)" value={note} onChange={(e) => setNote(e.target.value)} />
              </div>

              {settings?.textReceipts && !offlineOnly && (
                <label className="row" style={{ gap: "var(--space-3)" }}>
                  <Switch checked={textGuardian} onChange={setTextGuardian} label="Text the receipt to the payer" />
                  <span>Text the receipt and the new balance to the payer</span>
                </label>
              )}

              <div className="row-between">
                <span className="subtle">
                  {after !== null && `After this payment: ${balanceText(after)}`}
                </span>
                <Button
                  variant="primary"
                  icon={<Receipt size={15} />}
                  loading={busy}
                  disabled={!shillings || (methodInfo.needsReference && !reference.trim())}
                  onClick={() => void record()}
                >
                  Record and print receipt
                </Button>
              </div>
            </>
          )}
        </div>
      </Card>

      <Card title="SchoolPay" subtitle="Payments parents made through SchoolPay, from the statement">
        <div className="row-between">
          <span className="subtle">Download the statement from the SchoolPay portal and turn it into receipts in one step.</span>
          <Button icon={<FileSpreadsheet size={15} />} onClick={() => setImporting(true)}>
            Import SchoolPay statement
          </Button>
        </div>
      </Card>
      {importing && <SchoolPayImportModal onClose={() => setImporting(false)} onDone={() => setImporting(false)} />}

      {last && (
        <Card title="Last receipt" subtitle="Print again if the printer jammed">
          <div className="row-between">
            <span>
              <strong>{last.receiptLabel}</strong> • {last.studentName} • {ugx(last.amount)}
            </span>
            <Button
              size="sm"
              icon={<Printer size={14} />}
              onClick={() => api.buildFeeReceipt(last.id).then((e) => onPrint(<FeeReceiptSheet envelope={e} />)).catch(reportError)}
            >
              Print again
            </Button>
          </div>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Balances: who owes, and each learner's account
// ---------------------------------------------------------------------------

function Balances({ onPrint }: { onPrint: Print }) {
  const reportError = useStore((state) => state.reportError);
  const [setup, setSetup] = useState<FeeSetup | null>(null);
  const [termId, setTermId] = useState<string | null>(null);
  const [classId, setClassId] = useState<string>("");
  const [list, setList] = useState<FeeAccountList | null>(null);
  const [filter, setFilter] = useState<"all" | "owing" | "promised" | "cleared">("owing");
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [invoices, setInvoices] = useState(false);
  const [reminding, setReminding] = useState(false);
  const can = useCan();
  const offlineOnly = useStore((state) => state.offlineOnly);

  useEffect(() => {
    api
      .getFeeSetup(null)
      .then((s) => {
        setSetup(s);
        setTermId(s.termId);
      })
      .catch(reportError);
  }, [reportError]);

  const load = useCallback(() => {
    if (!termId) return;
    setList(null);
    api.listFeeAccounts(termId, classId || null).then(setList).catch(reportError);
  }, [termId, classId, reportError]);
  useEffect(load, [load]);

  const rows = useMemo(() => {
    if (!list) return [];
    const q = search.trim().toLowerCase();
    return list.rows.filter((r) => {
      if (q && !r.fullName.toLowerCase().includes(q) && !r.regNumber.toLowerCase().includes(q)) return false;
      if (filter === "owing") return r.balance > 0;
      if (filter === "promised") return r.promise !== null;
      if (filter === "cleared") return r.balance <= 0;
      return true;
    });
  }, [list, filter, search]);

  if (!setup) return <Loading label="Loading fees" />;
  if (setup.items.length === 0) {
    return (
      <EmptyState icon={<Banknote size={22} />} title="No fees set yet">
        Open Fee structure, add what the school charges (Tuition, Lunch…) and the amount for each class.
      </EmptyState>
    );
  }

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <div className="row" style={{ flexWrap: "wrap", gap: "var(--space-3)" }}>
        <SelectInput label="Term" value={termId ?? ""} onChange={(e) => setTermId(e.target.value)}>
          {setup.terms.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </SelectInput>
        <SelectInput label="Class" value={classId} onChange={(e) => setClassId(e.target.value)}>
          <option value="">Whole school</option>
          {setup.classes.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </SelectInput>
        <div className="grow" />
        {can("send_messages") && !offlineOnly && (
          <Button icon={<MessageSquare size={15} />} onClick={() => setReminding(true)}>
            Text reminders
          </Button>
        )}
        <Button icon={<FileText size={15} />} onClick={() => setInvoices(true)}>
          Print invoices
        </Button>
        <Button
          icon={<Printer size={15} />}
          onClick={() =>
            api
              .buildFeeBalances(termId, classId || null)
              .then((e) => onPrint(<FeeBalancesSheet envelope={e} />))
              .catch(reportError)
          }
        >
          Print who owes
        </Button>
      </div>

      {!list ? (
        <Loading label="Working out balances" />
      ) : (
        <>
          <div className="grid grid-stats fees-stats">
            <StatTile label="Expected" value={ugx(list.totals.expected)} meta={`${list.totals.learners} learners`} />
            <StatTile
              label="Collected"
              value={ugx(list.totals.collected)}
              meta={
                list.totals.expected > 0
                  ? `${Math.round((list.totals.collected / list.totals.expected) * 100)}% of expected`
                  : undefined
              }
            />
            <StatTile label="Still owed" value={ugx(list.totals.outstanding)} meta={`${list.totals.owing} learners owe`} />
            <StatTile label="Paid ahead" value={ugx(list.totals.credit)} />
          </div>

          <Card flush>
            <div className="row" style={{ padding: "var(--space-3) var(--space-4)", gap: "var(--space-3)", flexWrap: "wrap" }}>
              {(
                [
                  ["owing", "Owe money"],
                  ["promised", "Promised to pay"],
                  ["cleared", "Cleared"],
                  ["all", "Everyone"],
                ] as const
              ).map(([id, label]) => (
                <button key={id} className={cx("chip", filter === id && "is-active")} onClick={() => setFilter(id)}>
                  {label}
                </button>
              ))}
              <div className="grow" />
              <input
                className="input"
                style={{ maxWidth: 240 }}
                placeholder="Find a learner"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>
            <div className="table-wrap" style={{ maxHeight: 520 }}>
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Learner</th>
                    <th>Class</th>
                    <th className="num">Brought forward</th>
                    <th className="num">This term</th>
                    <th className="num">Paid</th>
                    <th className="num">Balance</th>
                    <th>Agreed to pay</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <BalanceRow key={row.studentId} row={row} onOpen={() => setOpen(row.studentId)} />
                  ))}
                  {rows.length === 0 && (
                    <tr>
                      <td colSpan={7} className="subtle" style={{ textAlign: "center", padding: "var(--space-5)" }}>
                        Nobody here.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </Card>
        </>
      )}

      {open && <AccountModal studentId={open} onClose={() => setOpen(null)} onChanged={load} onPrint={onPrint} />}
      {reminding && (
        <TextPlanModal
          title="Text fee reminders"
          load={() => api.previewFeeReminders(0)}
          send={() => api.sendFeeReminders(0)}
          onClose={() => setReminding(false)}
        />
      )}
      {invoices && termId && (
        <InvoicesModal
          setup={setup}
          termId={termId}
          classId={classId}
          onClose={() => setInvoices(false)}
          onPrint={onPrint}
        />
      )}
    </div>
  );
}

function BalanceRow({ row, onOpen }: { row: FeeAccountRow; onOpen: () => void }) {
  return (
    <tr className="clickable" onClick={onOpen}>
      <td>
        <div style={{ fontWeight: 600 }}>{row.fullName}</div>
        <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
          {row.regNumber}
        </div>
      </td>
      <td>{row.className}</td>
      <td className="num">{row.broughtForward ? row.broughtForward.toLocaleString("en-US") : "—"}</td>
      <td className="num">{row.billed.toLocaleString("en-US")}</td>
      <td className="num">{row.paid.toLocaleString("en-US")}</td>
      <td className="num">
        <Badge tone={balanceTone(row.balance)}>{balanceText(row.balance)}</Badge>
      </td>
      <td>
        {row.promise && (
          <span className={row.promise.overdue ? "tone-danger" : undefined}>
            {ugx(row.promise.amount)} by {shortDate(row.promise.dueOn)}
            {row.promise.overdue ? " (late)" : ""}
          </span>
        )}
      </td>
    </tr>
  );
}

function InvoicesModal({
  setup,
  termId,
  classId,
  onClose,
  onPrint,
}: {
  setup: FeeSetup;
  termId: string;
  classId: string;
  onClose: () => void;
  onPrint: Print;
}) {
  const reportError = useStore((state) => state.reportError);
  const [cls, setCls] = useState(classId);
  const [onlyOwing, setOnlyOwing] = useState(true);
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      title="Print invoices"
      description="One per learner, two to a page: what they owe, line by line, and how to pay."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            icon={<Printer size={15} />}
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .buildFeeInvoices({ termId, classId: cls || null, studentIds: null, onlyOwing })
                .then((e) => onPrint(<FeeInvoiceSheets envelope={e} />))
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Prepare invoices
          </Button>
        </>
      }
    >
      <div className="stack">
        <SelectInput label="Class" value={cls} onChange={(e) => setCls(e.target.value)}>
          <option value="">Whole school</option>
          {setup.classes.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name} ({c.learners})
            </option>
          ))}
        </SelectInput>
        <label className="row" style={{ gap: "var(--space-3)" }}>
          <Switch checked={onlyOwing} onChange={setOnlyOwing} label="Only learners who owe" />
          <span>Only learners who owe something</span>
        </label>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// One learner's account
// ---------------------------------------------------------------------------

function AccountModal({
  studentId,
  onClose,
  onChanged,
  onPrint,
}: {
  studentId: string;
  onClose: () => void;
  onChanged: () => void;
  onPrint: Print;
}) {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [detail, setDetail] = useState<FeeAccountDetail | null>(null);
  const [mode, setMode] = useState<"view" | "adjust" | "promise">("view");
  const [voiding, setVoiding] = useState<PaymentRow | null>(null);

  const load = useCallback(() => {
    api.getFeeAccount(studentId).then(setDetail).catch(reportError);
  }, [studentId, reportError]);
  useEffect(load, [load]);

  const changed = () => {
    load();
    onChanged();
  };

  return (
    <Modal
      open
      wide
      title={detail ? detail.learner.fullName : "Loading"}
      description={detail ? `${detail.learner.className ?? "Left"} • ${detail.learner.regNumber}` : undefined}
      onClose={onClose}
      footer={
        detail && (
          <>
            <Button
              icon={<Printer size={15} />}
              onClick={() => api.buildFeeStatement(studentId).then((e) => onPrint(<FeeStatementSheet envelope={e} />)).catch(reportError)}
            >
              Print statement
            </Button>
            {can("manage_fees") && (
              <>
                <Button icon={<HandCoins size={15} />} onClick={() => setMode("adjust")}>
                  Bursary or charge
                </Button>
                <Button icon={<CalendarClock size={15} />} onClick={() => setMode("promise")}>
                  Promise to pay
                </Button>
              </>
            )}
          </>
        )
      }
    >
      {!detail ? (
        <Loading label="Loading the account" />
      ) : mode === "adjust" ? (
        <AdjustmentForm studentId={studentId} onDone={() => { setMode("view"); changed(); }} onCancel={() => setMode("view")} />
      ) : mode === "promise" ? (
        <PromiseForm studentId={studentId} balance={detail.balance} onDone={() => { setMode("view"); changed(); }} onCancel={() => setMode("view")} />
      ) : (
        <div className="stack">
          <div className="fees-owes">
            <div>
              <span>Owes now</span>
              <strong className={`tone-${balanceTone(detail.balance)}`}>{balanceText(detail.balance)}</strong>
            </div>
            <div>
              <span>Parent</span>
              <strong>{detail.learner.guardianName ?? "—"}</strong>
            </div>
            <div>
              <span>Phone</span>
              <strong>{detail.learner.guardianPhone ?? "—"}</strong>
            </div>
          </div>

          {detail.terms
            .filter((term) => term.lines.length > 0 || term.paid !== 0 || term.broughtForward !== 0)
            .reverse()
            .slice(0, 3)
            .map((term) => (
            <div key={term.termId} className="fees-term">
              <div className="row-between">
                <strong>
                  {term.termName} {term.yearLabel}
                  {term.className ? ` • ${term.className}` : ""}
                </strong>
                <span className="subtle">Balance at end: {ugx(term.balance)}</span>
              </div>
              <table className="table table-compact">
                <tbody>
                  {term.broughtForward !== 0 && (
                    <tr>
                      <td>Brought forward</td>
                      <td className="num">{ugx(term.broughtForward)}</td>
                    </tr>
                  )}
                  {term.lines.map((line, i) => (
                    <tr key={i}>
                      <td>{line.label}</td>
                      <td className="num">{ugx(line.amount)}</td>
                    </tr>
                  ))}
                  <tr>
                    <td>Paid</td>
                    <td className="num">{ugx(-term.paid)}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          ))}

          <div>
            <div className="section-title">Receipts</div>
            {detail.payments.length === 0 ? (
              <p className="subtle">No payments yet.</p>
            ) : (
              <table className="table table-compact">
                <tbody>
                  {detail.payments.map((p) => (
                    <tr key={p.id} className={p.status === "voided" ? "is-voided" : undefined}>
                      <td>
                        <strong>{p.receiptLabel}</strong>
                        {p.status === "voided" && <Badge tone="danger">Cancelled</Badge>}
                      </td>
                      <td>{shortDate(p.paidOn)}</td>
                      <td>
                        {p.methodLabel}
                        {p.reference ? ` • ${p.reference}` : ""}
                      </td>
                      <td className="num">{ugx(p.amount)}</td>
                      <td style={{ whiteSpace: "nowrap", textAlign: "right" }}>
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<Printer size={14} />}
                          title="Print this receipt"
                          onClick={() => api.buildFeeReceipt(p.id).then((e) => onPrint(<FeeReceiptSheet envelope={e} />)).catch(reportError)}
                        />
                        {p.status === "valid" && can("void_receipts") && (
                          <Button size="sm" variant="ghost" icon={<Ban size={14} />} title="Cancel this receipt" onClick={() => setVoiding(p)} />
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          {detail.adjustments.length > 0 && (
            <div>
              <div className="section-title">Bursaries, discounts and charges</div>
              <table className="table table-compact">
                <tbody>
                  {detail.adjustments.map((a) => (
                    <tr key={a.id} className={a.status === "cancelled" ? "is-voided" : undefined}>
                      <td>
                        {a.label}
                        {a.feeItemName ? ` on ${a.feeItemName}` : ""}: {a.reason}
                        {a.status === "cancelled" && <Badge tone="neutral">Cancelled</Badge>}
                      </td>
                      <td>{a.termName}</td>
                      <td className="num">{a.amount !== null ? ugx(a.amount) : `${a.percent}%`}</td>
                      <td style={{ textAlign: "right" }}>
                        {a.status === "active" && can("manage_fees") && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                              const reason = window.prompt("Why is this being cancelled?");
                              if (!reason) return;
                              api.cancelFeeAdjustment(a.id, reason).then(changed).catch(reportError);
                            }}
                          >
                            Cancel
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {detail.promises.length > 0 && (
            <div>
              <div className="section-title">Promises to pay</div>
              <table className="table table-compact">
                <tbody>
                  {detail.promises.map((p) => (
                    <tr key={p.id}>
                      <td>
                        {ugx(p.amount)} by {shortDate(p.dueOn)}
                        {p.promisedBy ? ` (${p.promisedBy})` : ""}
                      </td>
                      <td>
                        <Badge tone={p.status === "kept" ? "success" : p.status === "open" ? (p.overdue ? "danger" : "info") : "neutral"}>
                          {p.status === "open" && p.overdue ? "late" : p.status}
                        </Badge>
                      </td>
                      <td style={{ textAlign: "right" }}>
                        {p.status === "open" && can("manage_fees") && (
                          <>
                            <Button size="sm" variant="ghost" onClick={() => api.closeFeePromise(p.id, "kept").then(changed).catch(reportError)}>
                              Kept
                            </Button>
                            <Button size="sm" variant="ghost" onClick={() => api.closeFeePromise(p.id, "broken").then(changed).catch(reportError)}>
                              Broken
                            </Button>
                          </>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {voiding && (
        <VoidModal
          payment={voiding}
          onClose={() => setVoiding(null)}
          onDone={() => {
            toast("success", `${voiding.receiptLabel} cancelled. It stays on record.`);
            setVoiding(null);
            changed();
          }}
        />
      )}
    </Modal>
  );
}

function VoidModal({ payment, onClose, onDone }: { payment: PaymentRow; onClose: () => void; onDone: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      title={`Cancel ${payment.receiptLabel}?`}
      description={`${ugx(payment.amount)} from ${payment.studentName}, ${shortDate(payment.paidOn)}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button
            variant="danger"
            icon={<Ban size={15} />}
            loading={busy}
            disabled={reason.trim().length < 5}
            onClick={() => {
              setBusy(true);
              api
                .voidFeePayment(payment.id, reason.trim())
                .then(onDone)
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Cancel receipt
          </Button>
        </>
      }
    >
      <div className="stack">
        <Alert tone="warning">
          The receipt is not deleted. It stays numbered in the books, marked cancelled with your name and the reason,
          and the money comes off the learner's payments. If the amount was wrong, cancel it and record the right one.
        </Alert>
        <TextArea label="Why?" rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. Entered twice by mistake" />
      </div>
    </Modal>
  );
}

function AdjustmentForm({ studentId, onDone, onCancel }: { studentId: string; onDone: () => void; onCancel: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [kind, setKind] = useState<AdjustmentKind>("bursary");
  const [byPercent, setByPercent] = useState(false);
  const [value, setValue] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const info = ADJUSTMENT_KINDS.find((k) => k.id === kind)!;
  const percent = byPercent && info.percentAllowed;
  const number = percent ? Number(value) : parseShillings(value);
  const valid = number !== null && Number.isFinite(number) && number > 0 && (!percent || number <= 100) && reason.trim() !== "";

  return (
    <div className="stack">
      <div className="field">
        <span className="field-label">What kind</span>
        <div className="row" style={{ flexWrap: "wrap" }}>
          {ADJUSTMENT_KINDS.map((k) => (
            <button key={k.id} className={cx("pick", kind === k.id && "is-selected")} style={{ width: "auto" }} onClick={() => setKind(k.id)} title={k.hint}>
              {k.label}
            </button>
          ))}
        </div>
        <span className="field-hint">{info.hint}</span>
      </div>
      {info.percentAllowed && (
        <label className="row" style={{ gap: "var(--space-3)" }}>
          <Switch checked={byPercent} onChange={setByPercent} label="A percentage of the bill" />
          <span>A percentage of this term's bill</span>
        </label>
      )}
      <div className="grid-form">
        <TextInput
          label={percent ? "Percent" : "Amount (UGX)"}
          inputMode="decimal"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          hint={!percent && number ? ugx(number) : undefined}
        />
        <TextInput label="Reason (prints on the invoice)" value={reason} onChange={(e) => setReason(e.target.value)} />
      </div>
      <div className="row" style={{ justifyContent: "flex-end" }}>
        <Button onClick={onCancel}>Back</Button>
        <Button
          variant="primary"
          loading={busy}
          disabled={!valid}
          onClick={() => {
            setBusy(true);
            api
              .addFeeAdjustment({
                studentIds: [studentId],
                termId: null,
                feeItemId: null,
                kind,
                amount: percent ? null : number,
                percent: percent ? number : null,
                reason: reason.trim(),
              })
              .then(() => {
                toast("success", `${info.label} added.`);
                onDone();
              })
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          Add
        </Button>
      </div>
    </div>
  );
}

function PromiseForm({
  studentId,
  balance,
  onDone,
  onCancel,
}: {
  studentId: string;
  balance: number;
  onDone: () => void;
  onCancel: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [amount, setAmount] = useState(balance > 0 ? String(balance) : "");
  const [dueOn, setDueOn] = useState("");
  const [promisedBy, setPromisedBy] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const shillings = parseShillings(amount);
  return (
    <div className="stack">
      <Alert tone="info">
        The Ministry asks schools to agree a payment plan with parents rather than send learners home. Record the
        agreement here: it shows on the balances list and the invoice, and is marked kept when the money comes in.
      </Alert>
      <div className="grid-form">
        <TextInput label="Amount (UGX)" value={amount} onChange={(e) => setAmount(e.target.value)} hint={shillings ? ugx(shillings) : undefined} />
        <TextInput label="By" type="date" min={today()} value={dueOn} onChange={(e) => setDueOn(e.target.value)} />
        <TextInput label="Who promised" value={promisedBy} onChange={(e) => setPromisedBy(e.target.value)} />
        <TextInput label="Note" value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
      <div className="row" style={{ justifyContent: "flex-end" }}>
        <Button onClick={onCancel}>Back</Button>
        <Button
          variant="primary"
          loading={busy}
          disabled={!shillings || !dueOn}
          onClick={() => {
            if (!shillings) return;
            setBusy(true);
            api
              .addFeePromise({ studentId, amount: shillings, dueOn, promisedBy: promisedBy.trim() || null, note: note.trim() || null })
              .then(onDone)
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          Save promise
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

function CollectionsTab({ onPrint }: { onPrint: Print }) {
  const reportError = useStore((state) => state.reportError);
  const [from, setFrom] = useState(today());
  const [to, setTo] = useState(today());
  const [data, setData] = useState<Collections | null>(null);

  useEffect(() => {
    if (!from || !to || from > to) return;
    setData(null);
    api.feeCollections(from, to).then(setData).catch(reportError);
  }, [from, to, reportError]);

  const quick = (days: number) => {
    const start = new Date();
    start.setDate(start.getDate() - days);
    setFrom(start.toLocaleDateString("en-CA"));
    setTo(today());
  };

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <div className="row" style={{ flexWrap: "wrap", gap: "var(--space-3)", alignItems: "flex-end" }}>
        <TextInput label="From" type="date" value={from} max={to} onChange={(e) => setFrom(e.target.value)} />
        <TextInput label="To" type="date" value={to} min={from} max={today()} onChange={(e) => setTo(e.target.value)} />
        <Button size="sm" variant="ghost" onClick={() => quick(0)}>
          Today
        </Button>
        <Button size="sm" variant="ghost" onClick={() => quick(6)}>
          Last 7 days
        </Button>
        <Button size="sm" variant="ghost" onClick={() => quick(29)}>
          Last 30 days
        </Button>
        <div className="grow" />
        <Button
          icon={<Printer size={15} />}
          onClick={() => api.buildFeeCollections(from, to).then((e) => onPrint(<FeeCollectionsSheet envelope={e} />)).catch(reportError)}
        >
          Print for banking
        </Button>
      </div>

      {!data ? (
        <Loading label="Adding up" />
      ) : (
        <>
          <div className="grid grid-stats fees-stats">
            <StatTile label="Total" value={ugx(data.totals.total)} meta={`${data.totals.receipts} receipt(s)`} />
            <StatTile label="Cash" value={ugx(data.totals.cash)} meta="Should be in the cash box or banked" />
            <StatTile label="Mobile Money" value={ugx(data.totals.mobileMoney)} />
            <StatTile label="Bank and SchoolPay" value={ugx(data.totals.bank + data.totals.schoolpay)} />
          </div>
          {data.voided.length > 0 && (
            <Alert tone="warning" title={`${data.voided.length} receipt(s) cancelled in these dates`}>
              {data.voided.map((p) => `${p.receiptLabel} (${ugx(p.amount)}, ${p.studentName}): ${p.voidReason}`).join(" • ")}
            </Alert>
          )}
          <Card flush title="Receipts">
            <div className="table-wrap" style={{ maxHeight: 460 }}>
              <table className="table table-compact">
                <thead>
                  <tr>
                    <th>Receipt</th>
                    <th>Date</th>
                    <th>Learner</th>
                    <th>Method</th>
                    <th>Received by</th>
                    <th className="num">Amount</th>
                  </tr>
                </thead>
                <tbody>
                  {data.payments.map((p) => (
                    <tr
                      key={p.id}
                      className="clickable"
                      onClick={() => api.buildFeeReceipt(p.id).then((e) => onPrint(<FeeReceiptSheet envelope={e} />)).catch(reportError)}
                    >
                      <td>{p.receiptLabel}</td>
                      <td>{shortDate(p.paidOn)}</td>
                      <td>
                        {p.studentName} <span className="subtle">{p.className ?? ""}</span>
                      </td>
                      <td>
                        {p.methodLabel}
                        {p.reference ? <span className="subtle"> {p.reference}</span> : null}
                      </td>
                      <td>{p.receivedByName ?? ""}</td>
                      <td className="num">{ugx(p.amount)}</td>
                    </tr>
                  ))}
                  {data.payments.length === 0 && (
                    <tr>
                      <td colSpan={6} className="subtle" style={{ textAlign: "center", padding: "var(--space-5)" }}>
                        Nothing received in these dates.
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
  );
}

// ---------------------------------------------------------------------------
// The fee structure
// ---------------------------------------------------------------------------

function Structure() {
  const can = useCan();
  const editable = can("manage_fees");
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [setup, setSetup] = useState<FeeSetup | null>(null);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [adding, setAdding] = useState(false);
  const [copying, setCopying] = useState(false);
  const [optins, setOptins] = useState<{ itemId: string; classId: string } | null>(null);

  const load = useCallback(
    (termId: string | null) => {
      api
        .getFeeSetup(termId)
        .then((s) => {
          setSetup(s);
          const next: Record<string, string> = {};
          for (const r of s.rates) next[`${r.feeItemId}|${r.classId}`] = r.amount ? String(r.amount) : "";
          setDraft(next);
        })
        .catch(reportError);
    },
    [reportError],
  );
  useEffect(() => load(null), [load]);

  if (!setup) return <Loading label="Loading the fee structure" />;

  const original = (key: string) => {
    const [item = "", cls = ""] = key.split("|");
    const rate = setup.rates.find((r) => r.feeItemId === item && r.classId === cls);
    return rate?.amount ? String(rate.amount) : "";
  };
  const dirty = Object.keys(draft).filter((key) => (draft[key] ?? "") !== original(key));
  const invalid = dirty.some((key) => (draft[key] ?? "") !== "" && parseShillings(draft[key] ?? "") === null);
  const classTotal = (classId: string) =>
    setup.items
      .filter((i) => i.kind === "termly")
      .reduce((sum, i) => sum + (parseShillings(draft[`${i.id}|${classId}`] ?? "") ?? 0), 0);

  async function save() {
    if (!setup) return;
    setBusy(true);
    try {
      const rates = dirty.map((key) => {
        const [feeItemId = "", classId = ""] = key.split("|");
        return { feeItemId, classId, amount: parseShillings(draft[key] ?? "") ?? 0 };
      });
      const changed = await api.saveFeeRates(setup.termId, rates);
      toast("success", `${changed} fee(s) saved. Every balance has been worked out again.`);
      load(setup.termId);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  const term = setup.terms.find((t) => t.id === setup.termId);

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <div className="row" style={{ flexWrap: "wrap", gap: "var(--space-3)", alignItems: "flex-end" }}>
        <SelectInput label="Term" value={setup.termId} onChange={(e) => load(e.target.value)}>
          {setup.terms.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
              {t.status === "planning" ? " (not open yet)" : ""}
            </option>
          ))}
        </SelectInput>
        <div className="grow" />
        {editable && (
          <>
            <Button icon={<Copy size={15} />} onClick={() => setCopying(true)}>
              Copy from another term
            </Button>
            <Button icon={<Plus size={15} />} onClick={() => setAdding(true)}>
              Add a fee
            </Button>
            <Button variant="primary" icon={<Save size={15} />} loading={busy} disabled={dirty.length === 0 || invalid} onClick={() => void save()}>
              Save {dirty.length > 0 ? `(${dirty.length})` : ""}
            </Button>
          </>
        )}
      </div>

      {term?.status === "planning" && (
        <Alert tone="info">
          This term has not opened yet. Setting its fees now lets report cards print next term's fees; nobody is
          billed until the term opens.
        </Alert>
      )}

      {setup.items.length === 0 ? (
        <EmptyState icon={<Banknote size={22} />} title="Add what the school charges">
          Start with Tuition. Add Lunch, Transport or Boarding as fees only some learners pay.
        </EmptyState>
      ) : (
        <Card flush>
          <div className="table-wrap">
            <table className="table table-compact fees-grid">
              <thead>
                <tr>
                  <th>Fee</th>
                  {setup.classes.map((c) => (
                    <th key={c.id} className="num">
                      {c.name}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {setup.items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{item.name}</div>
                      <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
                        {item.kind === "termly" ? "Everyone in the class" : "Only those signed up"}
                        {editable && (
                          <button
                            className="link-button"
                            onClick={() => {
                              if (!window.confirm(`Stop charging ${item.name}? Past bills keep it.`)) return;
                              api.retireFeeItem(item.id).then(() => load(setup.termId)).catch(reportError);
                            }}
                          >
                            Remove
                          </button>
                        )}
                      </div>
                    </td>
                    {setup.classes.map((c) => {
                      const key = `${item.id}|${c.id}`;
                      const count = setup.optinCounts.find((o) => o.feeItemId === item.id && o.classId === c.id)?.amount ?? 0;
                      return (
                        <td key={c.id} className="num">
                          <input
                            className={cx("input input-sm fees-cell", (draft[key] ?? "") !== original(key) && "is-dirty")}
                            inputMode="numeric"
                            disabled={!editable}
                            value={draft[key] ?? ""}
                            placeholder="0"
                            onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
                          />
                          {item.kind === "optional" && (draft[key] ?? "") !== "" && (
                            <button className="link-button" onClick={() => setOptins({ itemId: item.id, classId: c.id })}>
                              <Users size={11} /> {count} of {c.learners}
                            </button>
                          )}
                        </td>
                      );
                    })}
                  </tr>
                ))}
                <tr className="fees-total">
                  <td>Every learner pays</td>
                  {setup.classes.map((c) => (
                    <td key={c.id} className="num">
                      {classTotal(c.id).toLocaleString("en-US")}
                    </td>
                  ))}
                </tr>
              </tbody>
            </table>
          </div>
        </Card>
      )}

      {adding && <AddItemModal onClose={() => setAdding(false)} onSaved={() => { setAdding(false); load(setup.termId); }} />}
      {copying && (
        <CopyModal
          setup={setup}
          onClose={() => setCopying(false)}
          onDone={() => {
            setCopying(false);
            load(setup.termId);
          }}
        />
      )}
      {optins && (
        <OptinsModal
          termId={setup.termId}
          item={setup.items.find((i) => i.id === optins.itemId)!}
          classId={optins.classId}
          className={setup.classes.find((c) => c.id === optins.classId)?.name ?? ""}
          onClose={() => {
            setOptins(null);
            load(setup.termId);
          }}
        />
      )}
    </div>
  );
}

function AddItemModal({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<FeeItemKind>("termly");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      title="Add a fee"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!name.trim()}
            onClick={() => {
              setBusy(true);
              api
                .saveFeeItem({ id: null, name: name.trim(), kind, position: null })
                .then(onSaved)
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Add
          </Button>
        </>
      }
    >
      <div className="stack">
        <TextInput label="Name" autoFocus placeholder="e.g. Tuition, Lunch, Transport, Development" value={name} onChange={(e) => setName(e.target.value)} />
        <div className="field">
          <span className="field-label">Who pays it</span>
          <div className="row">
            <button className={cx("pick", kind === "termly" && "is-selected")} onClick={() => setKind("termly")}>
              Every learner in the class
            </button>
            <button className={cx("pick", kind === "optional" && "is-selected")} onClick={() => setKind("optional")}>
              Only those signed up (lunch, transport)
            </button>
          </div>
        </div>
      </div>
    </Modal>
  );
}

function CopyModal({ setup, onClose, onDone }: { setup: FeeSetup; onClose: () => void; onDone: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const sources = setup.terms.filter((t) => t.id !== setup.termId && t.hasRates);
  const [from, setFrom] = useState(sources[0]?.id ?? "");
  const [busy, setBusy] = useState(false);
  const target = setup.terms.find((t) => t.id === setup.termId)?.label ?? "this term";
  return (
    <Modal
      open
      title={`Copy fees into ${target}`}
      description="Every amount is copied; you can change any of them afterwards."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!from}
            onClick={() => {
              setBusy(true);
              api
                .copyFeeStructure(from, setup.termId)
                .then((n) => {
                  toast("success", `${n} fee(s) copied.`);
                  onDone();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Copy
          </Button>
        </>
      }
    >
      {sources.length === 0 ? (
        <p className="subtle">No other term has fees to copy yet.</p>
      ) : (
        <SelectInput label="Copy from" value={from} onChange={(e) => setFrom(e.target.value)}>
          {sources.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </SelectInput>
      )}
    </Modal>
  );
}

function OptinsModal({
  termId,
  item,
  classId,
  className,
  onClose,
}: {
  termId: string;
  item: { id: string; name: string };
  classId: string;
  className: string;
  onClose: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [rows, setRows] = useState<OptinRow[] | null>(null);
  const load = useCallback(() => {
    api.listFeeOptins(termId, item.id, classId).then(setRows).catch(reportError);
  }, [termId, item.id, classId, reportError]);
  useEffect(load, [load]);

  const set = (ids: string[], on: boolean) => api.setFeeOptins(termId, item.id, ids, on).then(load).catch(reportError);

  return (
    <Modal open wide title={`${item.name}: ${className}`} description="Tick everyone who takes it this term." onClose={onClose}>
      {!rows ? (
        <Loading />
      ) : (
        <div className="stack">
          <div className="row" style={{ gap: 8 }}>
            <Button size="sm" onClick={() => set(rows.map((r) => r.studentId), true)}>
              Everyone
            </Button>
            <Button size="sm" variant="ghost" onClick={() => set(rows.map((r) => r.studentId), false)}>
              Nobody
            </Button>
            <span className="subtle">
              {rows.filter((r) => r.taking).length} of {rows.length} take {item.name.toLowerCase()}
            </span>
          </div>
          <div className="optin-grid">
            {rows.map((r) => (
              <label key={r.studentId} className={cx("optin", r.taking && "is-on")}>
                <input type="checkbox" checked={r.taking} onChange={(e) => set([r.studentId], e.target.checked)} />
                {r.fullName}
              </label>
            ))}
          </div>
        </div>
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

function SettingsTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [settings, setSettings] = useState<FeeSettings | null>(null);
  const [limit, setLimit] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getFeeSettings()
      .then((s) => {
        setSettings(s);
        setLimit(s.autoBlockAbove === null ? "" : String(s.autoBlockAbove));
      })
      .catch(reportError);
  }, [reportError]);

  if (!settings) return <Loading />;
  const limitValue = limit.trim() === "" ? null : parseShillings(limit);

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)", maxWidth: 760 }}>
      <Card title="How parents pay" subtitle="Printed at the foot of every invoice">
        <TextArea
          rows={4}
          value={settings.paymentInstructions}
          placeholder={"e.g. Stanbic Bank, A/C 9030001234567, Rainbow N&P School\nMTN MoMo pay code 123456\nSchoolPay code: use the learner's payment code"}
          onChange={(e) => setSettings({ ...settings, paymentInstructions: e.target.value })}
        />
      </Card>
      <Card title="Report cards">
        <label className="row" style={{ gap: "var(--space-3)" }}>
          <Switch
            checked={settings.showOnReportCard}
            onChange={(on) => setSettings({ ...settings, showOnReportCard: on })}
            label="Print the fees balance and next term's fees on report cards"
          />
          <span>Print each learner's fees balance and next term's fees on the report card</span>
        </label>
      </Card>
      <Card title="Holding back report cards and exam permits">
        <p className="subtle" style={{ marginTop: 0 }}>
          Works only while the fees rule is on (Reports → Fees block). Learners above this balance are listed as held
          back instead of printed. Leave it empty to decide learner by learner. The Ministry of Education has told
          schools not to send learners home over fees; agree a payment plan with the parent instead.
        </p>
        <TextInput
          label="Hold back when the balance is above (UGX)"
          value={limit}
          onChange={(e) => setLimit(e.target.value)}
          hint={limitValue !== null ? ugx(limitValue) : "Empty: no automatic limit"}
          error={limit.trim() !== "" && limitValue === null ? "Numbers only" : undefined}
        />
      </Card>
      <Card title="Receipts by SMS">
        <label className="row" style={{ gap: "var(--space-3)" }}>
          <Switch
            checked={settings.textReceipts}
            onChange={(on) => setSettings({ ...settings, textReceipts: on })}
            label="Offer to text a receipt to the payer"
          />
          <span>Offer to text each receipt and the new balance to the payer. Each text costs one SMS.</span>
        </label>
      </Card>
      <div className="row" style={{ justifyContent: "flex-end" }}>
        <Button
          variant="primary"
          icon={<Settings2 size={15} />}
          loading={busy}
          disabled={limit.trim() !== "" && limitValue === null}
          onClick={() => {
            setBusy(true);
            api
              .saveFeeSettings({ ...settings, autoBlockAbove: limitValue })
              .then(() => toast("success", "Fees settings saved."))
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          Save settings
        </Button>
      </div>
    </div>
  );
}
