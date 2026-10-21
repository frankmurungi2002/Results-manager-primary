/**
 * The dashboard's "Getting started" list for a new school: each step ticked
 * from the school's own records, each one a button to the screen that does it.
 */

import { useEffect, useState } from "react";
import { CheckCircle2, Circle, X } from "lucide-react";

import { api } from "../lib/api";
import type { Checklist } from "../lib/types";
import { useStore, type ScreenId } from "../state/store";
import { Button, Progress } from "./ui";

export function GettingStarted() {
  const navigate = useStore((state) => state.navigate);
  const reportError = useStore((state) => state.reportError);
  const [list, setList] = useState<Checklist | null>(null);

  useEffect(() => {
    api.setupChecklist().then(setList).catch(() => setList(null));
  }, []);

  if (!list || !list.show) return null;
  const next = list.steps.find((s) => !s.done && !s.optional);

  return (
    <section className="card getting-started">
      <header className="card-header">
        <div>
          <div className="card-title">Getting started</div>
          <div className="card-subtitle">
            {list.done} of {list.total} done. Each tick comes from your records, so it shows what is really set up.
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          icon={<X size={14} />}
          onClick={() =>
            api
              .dismissSetupChecklist()
              .then(() => setList(null))
              .catch(reportError)
          }
        >
          Hide
        </Button>
      </header>
      <div style={{ padding: "0 var(--space-4) var(--space-2)" }}>
        <Progress value={Math.round((list.done / list.total) * 100)} />
      </div>
      <ol className="gs-list">
        {list.steps.map((s) => (
          <li key={s.id} className={s.done ? "gs-step is-done" : "gs-step"}>
            <span className="gs-icon" aria-hidden>
              {s.done ? <CheckCircle2 size={18} /> : <Circle size={18} />}
            </span>
            <span className="gs-text">
              <strong>
                {s.title}
                {s.optional ? <span className="subtle"> (optional)</span> : null}
              </strong>
              <span className="subtle">{s.detail}</span>
            </span>
            {!s.done && (
              <Button
                size="sm"
                variant={s.id === next?.id ? "primary" : "secondary"}
                onClick={() => navigate(s.screen as ScreenId, s.tab ? { tab: s.tab } : {})}
              >
                {s.id === next?.id ? "Do this next" : "Open"}
              </Button>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
