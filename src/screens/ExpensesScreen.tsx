/**
 * Spending, other income and banking, beside fees: what came in, what went
 * out, the surplus, and the cash that should be in the drawer now.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Ban, Landmark, Minus, Plus, Printer, Wallet } from "lucide-react";

import { api } from "../lib/api";
import { ugx, type CashEntry, type CashSummary, type DocumentEnvelope } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { CashSummarySheet, PrintPreview, VoucherSheet } from "../components/PrintDocument";
import { Alert, Badge, Button, Card, EmptyState, Loading, Modal, SelectInput, StatTile, TextInput, cx, formatDate } from "../components/ui";

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

function monthRange(offset: number): [string, string] {
  const now = new Date();
  const first = new Date(now.getFullYear(), now.getMonth() + offset, 1);
  const last = offset === 0 ? now : new Date(now.getFullYear(), now.getMonth() + offset + 1, 0);
  return [iso(first), iso(last)];
}

const METHODS = [
  { id: "cash", label: "Cash" },
  { id: "mobile_money", label: "Mobile Money" },
  { id: "bank", label: "Bank" },
  { id: "cheque", label: "Cheque" },
];

const KIND_LABEL: Record<string, string> = { expense: "Spending", income: "Other income", banking: "Banked" };

type Printing = { kind: "summary"; envelope: DocumentEnvelope<CashSummary> } | { kind: "voucher"; envelope: DocumentEnvelope<CashEntry> };

export function ExpensesScreen() {
  const can = useCan();
  const reportError = useStore((state) => state.reportError);
  const [[from, to], setRange] = useState<[string, string]>(monthRange(0));
  const [summary, setSummary] = useState<CashSummary | null>(null);
  const [recording, setRecording] = useState<"expense" | "income" | "banking" | null>(null);
  const [voiding, setVoiding] = useState<CashEntry | null>(null);
  const [printing, setPrinting] = useState<Printing | null>(null);
  const [filter, setFilter] = useState<"all" | "expense" | "income" | "banking">("all");

  const load = useCallback(() => {
    api.cashSummary(from, to).then(setSummary).catch(reportError);
  }, [from, to, reportError]);
  useEffect(load, [load]);

  if (printing) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview actions={<Button variant="ghost" icon={<ArrowLeft size={15} />} onClick={() => setPrinting(null)}>Back</Button>}>
          {printing.kind === "summary" ? <CashSummarySheet envelope={printing.envelope} /> : <VoucherSheet envelope={printing.envelope} />}
        </PrintPreview>
      </div>
    );
  }

  const mayRecord = can("manage_fees");
  const entries = summary ? summary.entries.filter((e) => filter === "all" || e.kind === filter) : [];

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Expenses</h1>
            <p className="page-description">
              Spending, other income and cash taken to the bank, each on a numbered voucher that is never edited or deleted. With fees,
              this gives the surplus and the cash that should be in the drawer.
            </p>
          </div>
          {mayRecord && (
            <div className="row" style={{ gap: 8 }}>
              <Button variant="primary" icon={<Minus size={15} />} onClick={() => setRecording("expense")}>
                Record spending
              </Button>
              <Button icon={<Plus size={15} />} onClick={() => setRecording("income")}>
                Other income
              </Button>
              <Button icon={<Landmark size={15} />} onClick={() => setRecording("banking")}>
                Bank cash
              </Button>
            </div>
          )}
        </div>

        <div className="row" style={{ gap: 12, alignItems: "end", flexWrap: "wrap" }}>
          <div style={{ width: 170 }}>
            <TextInput label="From" type="date" value={from} max={to} onChange={(e) => setRange([e.target.value, to])} />
          </div>
          <div style={{ width: 170 }}>
            <TextInput label="To" type="date" value={to} min={from} onChange={(e) => setRange([from, e.target.value])} />
          </div>
          <Button variant="ghost" onClick={() => setRange([iso(new Date()), iso(new Date())])}>
            Today
          </Button>
          <Button variant="ghost" onClick={() => setRange(monthRange(0))}>
            This month
          </Button>
          <Button variant="ghost" onClick={() => setRange(monthRange(-1))}>
            Last month
          </Button>
          <span className="grow" />
          <Button icon={<Printer size={15} />} onClick={() => api.buildCashSummary(from, to).then((envelope) => setPrinting({ kind: "summary", envelope })).catch(reportError)}>
            Print statement
          </Button>
        </div>

        {!summary ? (
          <Loading />
        ) : (
          <>
            <div className="grid grid-stats fees-stats" style={{ marginTop: "var(--space-4)" }}>
              <StatTile label="Money in" value={ugx(summary.income)} meta={`Fees ${ugx(summary.fees)}${summary.otherIncome ? `, other ${ugx(summary.otherIncome)}` : ""}`} />
              <StatTile label="Spent" value={ugx(summary.spent)} meta={summary.spending[0] ? `Most on ${summary.spending[0].category.toLowerCase()}` : undefined} />
              <StatTile
                label={summary.surplus >= 0 ? "Surplus" : "Shortfall"}
                value={<span style={{ color: summary.surplus >= 0 ? "var(--success)" : "var(--danger)" }}>{ugx(summary.surplus)}</span>}
              />
              <StatTile label="Cash at hand" value={ugx(summary.cashAtEnd)} meta={`${ugx(summary.cashAtStart)} at the start${summary.banked ? `, ${ugx(summary.banked)} banked` : ""}`} />
            </div>
            {summary.cashAtEnd < 0 && (
              <Alert tone="danger">
                The books show less than nothing in cash. Some cash that came in has not been receipted, or spending was recorded as cash by mistake.
              </Alert>
            )}

            <div className="grid" style={{ gridTemplateColumns: "minmax(0, 2fr) minmax(260px, 1fr)", gap: "var(--space-4)", alignItems: "start" }}>
              <Card
                flush
                title="Vouchers"
                actions={
                  <select className="input input-sm" value={filter} onChange={(e) => setFilter(e.target.value as typeof filter)} aria-label="Show">
                    <option value="all">Everything</option>
                    <option value="expense">Spending</option>
                    <option value="income">Other income</option>
                    <option value="banking">Banking</option>
                  </select>
                }
              >
                {entries.length === 0 ? (
                  <EmptyState icon={<Wallet size={20} />} title="Nothing in these dates">
                    {mayRecord ? "Record spending as it happens: each one gets a voucher to sign." : "The bursar records spending here."}
                  </EmptyState>
                ) : (
                  <div className="table-wrap">
                    <table className="table table-compact">
                      <thead>
                        <tr>
                          <th>Voucher</th>
                          <th>Date</th>
                          <th>What for</th>
                          <th className="num">Amount</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {entries.map((e) => (
                          <tr key={e.id} className={cx(e.status === "voided" && "is-quiet")}>
                            <td className="mono" style={{ whiteSpace: "nowrap" }}>
                              {e.voucher}
                              {e.status === "voided" && (
                                <div>
                                  <Badge tone="danger">Cancelled</Badge>
                                </div>
                              )}
                            </td>
                            <td style={{ whiteSpace: "nowrap" }}>{formatDate(e.onDate)}</td>
                            <td>
                              <div style={{ fontWeight: 600 }}>{e.description}</div>
                              <div className="subtle">
                                {KIND_LABEL[e.kind]}: {e.category}
                                {e.party ? `, ${e.kind === "income" ? "from" : "to"} ${e.party}` : ""}
                                {e.method !== "cash" ? `, ${METHODS.find((m) => m.id === e.method)?.label} ${e.reference ?? ""}` : ", cash"}
                                {e.voidReason ? `. Cancelled: ${e.voidReason}` : ""}
                              </div>
                            </td>
                            <td className="num" style={{ fontWeight: 600, whiteSpace: "nowrap", color: e.kind === "income" ? "var(--success)" : undefined }}>
                              {e.kind === "income" ? "+" : "−"}
                              {ugx(e.amount)}
                            </td>
                            <td className="right" style={{ whiteSpace: "nowrap" }}>
                              <Button size="sm" variant="ghost" icon={<Printer size={14} />} title="Print voucher" onClick={() => api.buildVoucher(e.id).then((envelope) => setPrinting({ kind: "voucher", envelope })).catch(reportError)} />
                              {can("void_receipts") && e.status === "valid" && (
                                <Button size="sm" variant="ghost" icon={<Ban size={14} />} title="Cancel voucher" onClick={() => setVoiding(e)} />
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </Card>
              <Card title="Where the money went">
                {summary.spending.length === 0 ? (
                  <p className="subtle" style={{ margin: 0 }}>No spending in these dates.</p>
                ) : (
                  <div className="stack" style={{ gap: 10 }}>
                    {summary.spending.map((c) => (
                      <div key={c.category}>
                        <div className="row-between" style={{ fontSize: 13 }}>
                          <span>{c.category}</span>
                          <strong>{ugx(c.amount)}</strong>
                        </div>
                        <div className="spend-bar">
                          <div style={{ width: `${(c.amount / summary.spent) * 100}%` }} />
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </Card>
            </div>
          </>
        )}
      </div>
      {recording && summary && (
        <RecordModal
          kind={recording}
          summary={summary}
          onClose={(saved) => {
            setRecording(null);
            if (saved) {
              load();
              if (saved.kind === "expense") api.buildVoucher(saved.id).then((envelope) => setPrinting({ kind: "voucher", envelope })).catch(reportError);
            }
          }}
        />
      )}
      {voiding && (
        <VoidModal
          entry={voiding}
          onClose={(done) => {
            setVoiding(null);
            if (done) load();
          }}
        />
      )}
    </div>
  );
}

function RecordModal({ kind, summary, onClose }: { kind: "expense" | "income" | "banking"; summary: CashSummary; onClose: (saved: CashEntry | null) => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const categories = kind === "income" ? summary.incomeCategories : summary.expenseCategories;
  const [category, setCategory] = useState(kind === "banking" ? "Cash banked" : categories[0] ?? "");
  const [description, setDescription] = useState("");
  const [party, setParty] = useState("");
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState("cash");
  const [reference, setReference] = useState("");
  const [onDate, setOnDate] = useState(iso(new Date()));
  const [busy, setBusy] = useState(false);
  const value = Number(amount.replace(/\D/g, "")) || 0;

  const save = () => {
    setBusy(true);
    api
      .recordCashEntry({
        kind,
        onDate,
        category,
        description,
        party: party || null,
        amount: value,
        method: kind === "banking" ? "cash" : method,
        reference: reference || null,
      })
      .then((entry) => {
        toast("success", `${entry.voucher} recorded.`);
        onClose(entry);
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  const title = kind === "expense" ? "Record spending" : kind === "income" ? "Other income" : "Cash taken to the bank";
  return (
    <Modal
      open
      title={title}
      description={
        kind === "expense"
          ? "A payment voucher prints for whoever took the money to sign."
          : kind === "income"
            ? "Money that is not school fees: items sold, a donation, hire of the hall."
            : "Moves cash from the drawer to the bank. It is not spending."
      }
      onClose={() => onClose(null)}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(null)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} disabled={value <= 0} onClick={save}>
            Save {value > 0 ? ugx(value) : ""}
          </Button>
        </>
      }
    >
      <div className="stack">
        {kind !== "banking" && (
          <>
            <SelectInput label={kind === "expense" ? "Spent on" : "Kind of income"} value={category} onChange={(e) => setCategory(e.target.value)}>
              {categories.map((c) => (
                <option key={c}>{c}</option>
              ))}
            </SelectInput>
            <TextInput label="What for" placeholder={kind === "expense" ? "e.g. 10 bags of maize flour" : "e.g. 12 sweaters sold"} value={description} onChange={(e) => setDescription(e.target.value)} />
            <TextInput label={kind === "expense" ? "Paid to" : "Received from"} placeholder="optional" value={party} onChange={(e) => setParty(e.target.value)} />
          </>
        )}
        <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr" }}>
          <TextInput label="Amount (UGX)" inputMode="numeric" value={amount} onChange={(e) => setAmount(e.target.value)} />
          <TextInput label="Date" type="date" value={onDate} max={iso(new Date())} onChange={(e) => setOnDate(e.target.value)} />
        </div>
        {kind !== "banking" ? (
          <div className="grid-form" style={{ gridTemplateColumns: "1fr 1fr" }}>
            <SelectInput label="How" value={method} onChange={(e) => setMethod(e.target.value)}>
              {METHODS.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </SelectInput>
            <TextInput label={method === "cash" ? "Reference (optional)" : "Transaction ID / slip / cheque no."} value={reference} onChange={(e) => setReference(e.target.value)} />
          </div>
        ) : (
          <TextInput label="Deposit slip number" value={reference} onChange={(e) => setReference(e.target.value)} />
        )}
        {kind !== "income" && method === "cash" && (
          <p className="subtle" style={{ margin: 0 }}>
            Cash at hand now: <strong>{ugx(summary.cashAtEnd)}</strong>
          </p>
        )}
      </div>
    </Modal>
  );
}

function VoidModal({ entry, onClose }: { entry: CashEntry; onClose: (done: boolean) => void }) {
  const reportError = useStore((state) => state.reportError);
  const [reason, setReason] = useState("");
  return (
    <Modal
      open
      title={`Cancel ${entry.voucher}?`}
      description={`${entry.description}, ${ugx(entry.amount)}. The voucher stays on record marked cancelled, with your reason.`}
      onClose={() => onClose(false)}
      footer={
        <>
          <span className="grow" />
          <Button variant="ghost" onClick={() => onClose(false)}>
            Keep it
          </Button>
          <Button variant="danger" disabled={reason.trim().length < 5} onClick={() => api.voidCashEntry(entry.id, reason).then(() => onClose(true)).catch(reportError)}>
            Cancel voucher
          </Button>
        </>
      }
    >
      <TextInput label="Why" placeholder="e.g. Entered twice" value={reason} onChange={(e) => setReason(e.target.value)} />
    </Modal>
  );
}
