/**
 * Settings → School, in the practice school only: clear it and open the
 * setup wizard for the real school. A backup of the practice data is kept.
 */

import { useState } from "react";
import { Eraser } from "lucide-react";

import { api } from "../lib/api";
import { useStore } from "../state/store";
import { Button, Card, TextInput } from "./ui";

export function StartRealSchoolCard() {
  const reportError = useStore((state) => state.reportError);
  const practice = useStore((state) => state.practice);
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  if (!practice) return null;
  return (
    <Card
      title="Start my real school"
      subtitle="Clears the demonstration school (a backup of it is kept) and opens the setup wizard for your real school. Everyone is signed out."
      footer={
        <div className="row-between" style={{ gap: 12, alignItems: "end", flexWrap: "wrap" }}>
          <div style={{ width: 220 }}>
            <TextInput label="Type START to confirm" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
          </div>
          <Button
            variant="danger"
            icon={<Eraser size={15} />}
            loading={busy}
            disabled={confirm.trim().toUpperCase() !== "START"}
            onClick={() => {
              setBusy(true);
              api
                .startRealSchool(confirm)
                .then(() => window.location.reload())
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Clear the demonstration school
          </Button>
        </div>
      }
    >
      <span className="field-hint">The demonstration school's backup stays in the backups list if you ever want it back.</span>
    </Card>
  );
}
