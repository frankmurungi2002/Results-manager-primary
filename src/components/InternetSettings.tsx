/**
 * Settings → Internet: one switch that keeps Phantom entirely offline.
 *
 * Everything a school does every day works with no internet. This panel says
 * plainly which few things use it, and lets a school turn all of them off.
 */

import { useCallback, useEffect, useState } from "react";
import { CloudOff, Globe, MessageSquare, RefreshCw, Wallet } from "lucide-react";

import { api } from "../lib/api";
import type { Connectivity } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Card, Loading, Modal, Button, Switch } from "./ui";

const ONLINE_FEATURES = [
  { icon: MessageSquare, label: "Sending SMS to parents and staff" },
  { icon: Wallet, label: "Checking the SMS credit balance" },
  { icon: RefreshCw, label: "Checking for and installing updates" },
  { icon: Globe, label: "Email invitations to teachers" },
];

export function InternetSettings({ children }: { children?: React.ReactNode }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const setOfflineOnly = useStore((state) => state.setOfflineOnly);
  const [connectivity, setConnectivity] = useState<Connectivity | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.getConnectivity().then(setConnectivity).catch(reportError);
  }, [reportError]);
  useEffect(load, [load]);

  const apply = (on: boolean) => {
    setBusy(true);
    api
      .setOfflineOnly(on)
      .then((next) => {
        setConnectivity(next);
        setOfflineOnly(next.offlineOnly);
        setConfirming(false);
        toast(
          "success",
          on ? "Phantom is now offline only. Nothing will use the internet." : "Internet features are allowed again.",
        );
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  };

  if (!connectivity) return <Loading label="Loading" />;

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <Card
        title="Offline only"
        subtitle="For a school that has no internet, or does not want this computer to use it"
        actions={
          <label className="row" style={{ gap: 8 }}>
            {connectivity.offlineOnly ? <Badge tone="info">On</Badge> : <Badge>Off</Badge>}
            <Switch
              checked={connectivity.offlineOnly}
              label="Offline only"
              disabled={busy}
              onChange={(on) => (on ? setConfirming(true) : apply(false))}
            />
          </label>
        }
      >
        <p className="subtle" style={{ marginTop: 0 }}>
          Marks, report cards, fees and receipts, attendance, printing, backups and everything else work without
          the internet whether this is on or off. Only these use it:
        </p>
        <ul className="plain-list">
          {ONLINE_FEATURES.map(({ icon: Icon, label }) => (
            <li key={label} className="row" style={{ gap: 8, padding: "4px 0" }}>
              <Icon size={15} /> {label}
            </li>
          ))}
        </ul>
        {connectivity.offlineOnly && (
          <Alert tone="info" title="Offline only is on">
            None of the above happens. Pass-outs and registers are still saved; parents are simply not texted.
          </Alert>
        )}
      </Card>

      {children}

      <Modal
        open={confirming}
        title="Use Phantom offline only?"
        description="Nothing will use the internet until you switch this off again."
        onClose={() => setConfirming(false)}
        footer={
          <>
            <Button onClick={() => setConfirming(false)}>Cancel</Button>
            <Button variant="primary" icon={<CloudOff size={15} />} loading={busy} onClick={() => apply(true)}>
              Go offline only
            </Button>
          </>
        }
      >
        {connectivity.queuedMessages > 0 ? (
          <Alert tone="warning" title={`${connectivity.queuedMessages} text(s) are waiting to be sent`}>
            They will be closed unsent, so that weeks-old messages never go out the day the internet is switched
            back on.
          </Alert>
        ) : (
          <p className="subtle">No texts are waiting, so nothing is lost.</p>
        )}
      </Modal>
    </div>
  );
}
