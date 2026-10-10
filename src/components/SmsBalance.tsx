/**
 * The SMS credit left, as the gateway last said it, with a button to ask
 * again. Every text sent through EgoSMS updates it for free.
 */

import { useEffect, useState } from "react";
import { RefreshCw, Wallet } from "lucide-react";

import { api } from "../lib/api";
import type { SmsBalance } from "../lib/types";
import { useStore } from "../state/store";
import { Badge, Button, relativeTime } from "./ui";

export function SmsBalanceLine({ compact = false }: { compact?: boolean }) {
  const reportError = useStore((state) => state.reportError);
  const offlineOnly = useStore((state) => state.offlineOnly);
  const [balance, setBalance] = useState<SmsBalance | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.getSmsBalance().then(setBalance).catch(() => setBalance(null));
  }, []);

  if (offlineOnly || !balance) return null;
  const amount =
    balance.amount === null ? "not checked yet" : `${balance.currency} ${Math.floor(balance.amount).toLocaleString("en-US")}`;

  return (
    <div className={compact ? "sms-balance sms-balance-compact" : "sms-balance"}>
      <Wallet size={15} />
      <span>
        SMS credit: <strong>{amount}</strong>
        {balance.textsLeft !== null && <span className="subtle"> (about {balance.textsLeft.toLocaleString()} texts)</span>}
        {balance.checkedAt && <span className="subtle"> • {relativeTime(balance.checkedAt)}</span>}
      </span>
      {balance.low && <Badge tone="danger">Low: top up</Badge>}
      <Button
        size="sm"
        variant="ghost"
        icon={<RefreshCw size={13} />}
        loading={busy}
        onClick={() => {
          setBusy(true);
          api
            .checkSmsBalance()
            .then(setBalance)
            .catch(reportError)
            .finally(() => setBusy(false));
        }}
      >
        Check now
      </Button>
    </div>
  );
}
