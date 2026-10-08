/**
 * FR-B11 — the subscription, as the school sees it: a banner when it needs
 * attention, and Settings → Subscription to read the school code and enter a
 * key. The backend decides everything; this only shows it.
 */

import { useCallback, useEffect, useState } from "react";
import { Copy, KeyRound, ShieldCheck, TriangleAlert } from "lucide-react";

import { api } from "../lib/api";
import type { LicenceStatus } from "../lib/types";
import { useCan, useStore } from "../state/store";
import { Alert, Badge, Button, Card, Loading, formatDate } from "./ui";

const PLAN: Record<string, string> = { term: "One term", year: "One year" };

/** Keeps the status fresh: a subscription can end while the window is open. */
export function useLicence(): [LicenceStatus | null, () => void] {
  const [status, setStatus] = useState<LicenceStatus | null>(null);
  const load = useCallback(() => {
    api.licenceStatus().then(setStatus).catch(() => undefined);
  }, []);
  useEffect(() => {
    load();
    const handle = window.setInterval(load, 60 * 60 * 1000);
    return () => window.clearInterval(handle);
  }, [load]);
  return [status, load];
}

function days(n: number): string {
  return n === 1 ? "1 day" : `${n} days`;
}

/** Shown across the top of every screen when the subscription needs attention. */
export function LicenceBanner({ status }: { status: LicenceStatus | null }) {
  const navigate = useStore((state) => state.navigate);
  const can = useCan();
  if (!status) return null;
  const left = status.daysLeft ?? 0;
  const fix = can("manage_school") ? (
    <button className="link" onClick={() => navigate("settings", { tab: "subscription" })}>
      Enter a key
    </button>
  ) : (
    <span>Tell the School Admin.</span>
  );

  if (status.readOnly) {
    return (
      <div className="licence-banner is-locked" role="alert">
        <TriangleAlert size={16} />
        <span>
          <strong>The subscription has ended. Phantom is read-only.</strong> Everything can be viewed, printed and
          backed up; nothing can be added or changed until a renewal key is entered. {fix}
        </span>
      </div>
    );
  }
  if (status.state === "grace") {
    return (
      <div className="licence-banner is-warn" role="status">
        <TriangleAlert size={16} />
        <span>
          <strong>The subscription has ended.</strong> Phantom keeps working for {days(left)} more, then becomes
          read-only. {fix}
        </span>
      </div>
    );
  }
  if ((status.state === "trial" || status.state === "active") && left <= 14) {
    return (
      <div className="licence-banner is-info" role="status">
        <KeyRound size={16} />
        <span>
          {status.state === "trial" ? "The trial" : "The subscription"} ends in {days(left)}
          {status.until ? ` (${formatDate(status.until)})` : ""}. {fix}
        </span>
      </div>
    );
  }
  return null;
}

export function SubscriptionSettings() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const can = useCan();
  const [status, reload] = useLicence();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);

  if (!status) return <Loading label="Checking the subscription" />;

  const stateLabel: Record<LicenceStatus["state"], [string, "success" | "warning" | "danger" | "info" | "neutral"]> = {
    pilot: ["Pilot — no subscription needed", "neutral"],
    trial: ["Trial", "info"],
    active: ["Active", "success"],
    grace: ["Ended — grace period", "warning"],
    expired: ["Ended — read-only", "danger"],
  };
  const [label, tone] = stateLabel[status.state];

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <Card title="Subscription" actions={<Badge tone={tone}>{label}</Badge>}>
        <div className="sub-grid">
          <div className="sub-fact">
            <span>School code</span>
            <strong className="mono">{status.schoolCode.replace(/(.{5})(.{5})/, "$1-$2")}</strong>
            <Button
              size="sm"
              variant="ghost"
              icon={<Copy size={13} />}
              onClick={() => {
                void navigator.clipboard?.writeText(status.schoolCode);
                toast("success", "School code copied.");
              }}
            >
              Copy
            </Button>
          </div>
          {status.plan && (
            <div className="sub-fact">
              <span>Plan</span>
              <strong>{PLAN[status.plan] ?? status.plan}</strong>
              {status.keyId && <span className="subtle">Ref {status.keyId}</span>}
            </div>
          )}
          {status.until && (
            <div className="sub-fact">
              <span>{status.state === "grace" || status.state === "expired" ? "Full use until" : "Paid until"}</span>
              <strong>{formatDate(status.until)}</strong>
              {status.daysLeft !== null && (
                <span className="subtle">
                  {status.daysLeft >= 0 ? `${days(status.daysLeft)} left` : `${days(-status.daysLeft)} ago`}
                </span>
              )}
            </div>
          )}
        </div>
        {status.clockRolledBack && (
          <Alert tone="warning" title="This computer's clock is behind">
            It shows an earlier date than Phantom has already seen, so the later date is being used. Set the correct
            date and time in Windows.
          </Alert>
        )}
      </Card>

      {status.state !== "pilot" && (
        <Card title="How to pay and renew" subtitle="Termly or yearly">
          <ol className="sub-steps">
            <li>Pay for a term or a year as agreed with Phantom School Manager.</li>
            <li>
              Send your <strong>school code</strong> (above) with the payment reference.
            </li>
            <li>You receive a licence key: a long line starting PSM1. Paste it below.</li>
          </ol>
          {can("manage_school") ? (
            <div className="stack">
              <textarea
                className="textarea mono"
                rows={4}
                aria-label="Licence key"
                placeholder="PSM1.eyJ2IjoxLCJzY2hvb2wiOi..."
                value={key}
                onChange={(event) => setKey(event.target.value)}
                spellCheck={false}
              />
              <Button
                variant="primary"
                icon={<ShieldCheck size={15} />}
                loading={busy}
                disabled={key.trim().length < 40}
                onClick={() => {
                  setBusy(true);
                  api
                    .installLicenceKey(key)
                    .then((next) => {
                      toast("success", `Key installed. Paid until ${formatDate(next.until)}.`);
                      setKey("");
                      reload();
                    })
                    .catch(reportError)
                    .finally(() => setBusy(false));
                }}
              >
                Install key
              </Button>
            </div>
          ) : (
            <p className="muted">Only the School Admin can enter a key.</p>
          )}
        </Card>
      )}
    </div>
  );
}
