/**
 * Settings → Internet & updates: the version, and installing a newer one.
 */

import { useEffect, useState } from "react";
import { Download, RefreshCw } from "lucide-react";

import { api } from "../lib/api";
import type { UpdateInfo } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Button, Card, relativeTime } from "./ui";

export function UpdatesCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState<"check" | "install" | null>(null);

  useEffect(() => {
    api.updateStatus().then(setInfo).catch(reportError);
  }, [reportError]);

  if (!info) return null;
  return (
    <Card
      title="Updates"
      subtitle={`This computer runs Termbook ${info.currentVersion}`}
      actions={info.availableVersion ? <Badge tone="info">{info.availableVersion} is ready</Badge> : undefined}
    >
      {!info.configured ? (
        <p className="subtle" style={{ margin: 0 }}>
          Automatic updates are not set up in this copy yet. The next version you install by hand will update itself after
          that.
        </p>
      ) : info.offlineOnly ? (
        <Alert tone="info">Offline only is on, so Termbook does not look for updates.</Alert>
      ) : (
        <div className="stack">
          {info.availableVersion ? (
            <Alert tone="info" title={`Termbook ${info.availableVersion} is available`}>
              {info.availableNotes || "Fixes and improvements."} Installing takes a backup first, then restarts Termbook:
              do it when nobody is entering marks.
            </Alert>
          ) : (
            <p className="subtle" style={{ margin: 0 }}>
              {info.lastChecked ? `Up to date. Last checked ${relativeTime(info.lastChecked)}.` : "Termbook checks once a day while it is open."}
            </p>
          )}
          <div className="row" style={{ gap: 8, justifyContent: "flex-end" }}>
            <Button
              icon={<RefreshCw size={15} />}
              loading={busy === "check"}
              onClick={() => {
                setBusy("check");
                api
                  .checkForUpdate()
                  .then((next) => {
                    setInfo(next);
                    if (!next.availableVersion) toast("success", "Termbook is up to date.");
                  })
                  .catch(reportError)
                  .finally(() => setBusy(null));
              }}
            >
              Check now
            </Button>
            {info.availableVersion && (
              <Button
                variant="primary"
                icon={<Download size={15} />}
                loading={busy === "install"}
                onClick={() => {
                  if (!window.confirm("Install now? Termbook will take a backup and restart.")) return;
                  setBusy("install");
                  api
                    .installUpdate()
                    .catch(reportError)
                    .finally(() => setBusy(null));
                }}
              >
                Install {info.availableVersion}
              </Button>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}
