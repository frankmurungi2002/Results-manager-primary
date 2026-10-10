/**
 * Settings → My account: save the log for support. Termbook writes what it
 * did and what failed to a log on this computer; this saves it as one text
 * file to send on WhatsApp or email when something goes wrong.
 */

import { useState } from "react";
import { LifeBuoy } from "lucide-react";

import { api } from "../lib/api";
import { useStore } from "../state/store";
import { keepExport } from "./SaveExport";
import { Button, Card } from "./ui";

export function SupportCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const appVersion = useStore((state) => state.appVersion);
  const [busy, setBusy] = useState(false);
  return (
    <Card
      title="Help and support"
      subtitle="When something goes wrong, save the log and send it to support. It records what Termbook did and what failed; it holds no marks, fees or passwords."
      actions={
        <Button
          icon={<LifeBuoy size={15} />}
          loading={busy}
          onClick={() => {
            setBusy(true);
            api
              .saveSupportLog()
              .then(keepExport)
              .then((where) => where && toast("success", `Saved to ${where}`))
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          Save the log for support
        </Button>
      }
    >
      <span className="field-hint">Termbook version {appVersion}</span>
    </Card>
  );
}
