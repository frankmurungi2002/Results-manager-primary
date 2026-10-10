/**
 * Check a document: is this receipt or report card really one the school
 * printed? Works offline. A QR scanner (which types what it reads) or the
 * receipt number and code typed off the paper.
 */

import { useEffect, useState } from "react";
import { BadgeCheck, ScanLine, ShieldAlert } from "lucide-react";

import { api } from "../lib/api";
import type { DocumentCheck } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Button, Card, Segmented, TextArea, TextInput } from "../components/ui";

export function VerifyScreen() {
  const reportError = useStore((state) => state.reportError);
  const [mode, setMode] = useState<"scan" | "type">("scan");
  const [scanned, setScanned] = useState("");
  const [receiptNo, setReceiptNo] = useState("");
  const [code, setCode] = useState("");
  const [result, setResult] = useState<DocumentCheck | null>(null);
  const [busy, setBusy] = useState(false);

  // A scanner types the whole QR, ending with the "TBK|" line (or "PSM|" on copies printed before the rename): check as soon
  // as that line is complete, so nobody has to press anything.
  useEffect(() => {
    const line = scanned.split(/\r?\n/).find((l) => /^(TBK|PSM)\|/.test(l.trim()));
    if (!line || !/\|[RC]-[0-9A-Z]{4}-[0-9A-Z]{4}-[0-9A-Z]{4}\s*$/i.test(line)) return;
    const timer = window.setTimeout(() => void check(() => api.checkDocument(scanned)), 150);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanned]);

  async function check(run: () => Promise<DocumentCheck>) {
    setBusy(true);
    try {
      setResult(await run());
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  const number = Number(receiptNo.replace(/\D/g, ""));

  return (
    <div className="page">
      <div className="page-inner" style={{ maxWidth: 820 }}>
        <div className="page-head">
          <div>
            <h1 className="page-title">Check a document</h1>
            <p className="page-description">
              Every receipt and report card Termbook prints carries a QR code and a short code. Check one here to be
              sure it is genuine and nothing on it was changed. No internet needed.
            </p>
          </div>
        </div>

        <Segmented
          value={mode}
          onChange={(m) => {
            setMode(m);
            setResult(null);
          }}
          options={[
            { value: "scan", label: "Scan the QR code" },
            { value: "type", label: "Type a receipt's code" },
          ]}
        />

        <div style={{ height: "var(--space-4)" }} />
        <Card>
          {mode === "scan" ? (
            <div className="stack">
              <p className="subtle" style={{ margin: 0 }}>
                Click in the box and scan with a QR scanner. A phone's camera also reads the code: paste what it shows.
              </p>
              <TextArea
                rows={5}
                autoFocus
                placeholder="The QR code's text appears here"
                value={scanned}
                onChange={(e) => {
                  setScanned(e.target.value);
                  setResult(null);
                }}
              />
              <div className="row" style={{ justifyContent: "flex-end", gap: 8 }}>
                <Button variant="ghost" onClick={() => { setScanned(""); setResult(null); }}>
                  Clear
                </Button>
                <Button variant="primary" icon={<ScanLine size={15} />} loading={busy} disabled={!scanned.trim()} onClick={() => void check(() => api.checkDocument(scanned))}>
                  Check
                </Button>
              </div>
            </div>
          ) : (
            <div className="stack">
              <div className="grid-form">
                <TextInput label="Receipt number" placeholder="RCT-000058" value={receiptNo} onChange={(e) => setReceiptNo(e.target.value)} />
                <TextInput label="Code printed beside the QR" placeholder="R-XXXX-XXXX-XXXX" value={code} onChange={(e) => setCode(e.target.value)} />
              </div>
              <div className="row" style={{ justifyContent: "flex-end" }}>
                <Button
                  variant="primary"
                  icon={<ScanLine size={15} />}
                  loading={busy}
                  disabled={!number || code.trim().length < 12}
                  onClick={() => void check(() => api.checkReceiptCode(number, code))}
                >
                  Check
                </Button>
              </div>
            </div>
          )}
        </Card>

        {result && (
          <div className={result.genuine ? "verify-result is-genuine" : "verify-result is-forged"}>
            <div className="verify-head">
              {result.genuine ? <BadgeCheck size={28} /> : <ShieldAlert size={28} />}
              <strong>{result.headline}</strong>
            </div>
            {result.warning && <Alert tone="danger">{result.warning}</Alert>}
            {result.facts.length > 0 && (
              <>
                <p className="subtle">What the school recorded. Compare each line with the paper:</p>
                <dl className="verify-facts">
                  {result.facts.map(([label, value]) => (
                    <div key={label}>
                      <dt>{label}</dt>
                      <dd>{value}</dd>
                    </div>
                  ))}
                </dl>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
