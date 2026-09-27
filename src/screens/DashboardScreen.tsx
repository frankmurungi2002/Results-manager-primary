/** The landing screen: where the school stands, and what needs doing next. */

import { useEffect, useState } from "react";
import {
  ClipboardList,
  DatabaseBackup,
  FileText,
  TriangleAlert,
  UserPlus,
} from "lucide-react";

import { api } from "../lib/api";
import type { DashboardSummary } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Progress,
  StatTile,
  TableSkeleton,
  formatDateTime,
  relativeTime,
} from "../components/ui";

export function DashboardScreen() {
  const session = useStore((state) => state.session);
  const navigate = useStore((state) => state.navigate);
  const reportError = useStore((state) => state.reportError);

  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api
      .dashboardSummary()
      .then(setSummary)
      .catch(reportError)
      .finally(() => setLoading(false));
  }, [reportError]);

  if (loading) {
    return (
      <div className="page">
        <div className="page-inner">
          <div className="grid grid-stats">
            {[0, 1, 2, 3].map((index) => (
              <div key={index} className="card stat">
                <div className="skeleton" style={{ height: 11, width: "50%" }} />
                <div
                  className="skeleton"
                  style={{ height: 28, width: "35%", marginTop: 12 }}
                />
              </div>
            ))}
          </div>
          <Card title="Recent activity">
            <TableSkeleton rows={5} columns={3} />
          </Card>
        </div>
      </div>
    );
  }

  if (!summary || !session) return null;

  const marksPercent =
    summary.marksExpectedThisTerm > 0
      ? Math.round(
          (summary.marksEnteredThisTerm / summary.marksExpectedThisTerm) * 100,
        )
      : 0;

  const backupStale =
    !summary.lastBackupAt ||
    Date.now() - new Date(summary.lastBackupAt).getTime() > 3 * 24 * 60 * 60 * 1000;

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">
              Good {greeting()}, {firstName(session.fullName)}
            </h1>
            <p className="page-description">
              {summary.currentTerm
                ? `${summary.currentTerm} of ${summary.academicYear} is open for marks entry.`
                : "No term is open yet. Open one from Terms & exams to start entering marks."}
            </p>
          </div>
          <div className="page-actions">
            <Button
              icon={<ClipboardList size={15} />}
              onClick={() => navigate("marks")}
            >
              Enter marks
            </Button>
            <Button
              variant="primary"
              icon={<FileText size={15} />}
              onClick={() => navigate("reports")}
            >
              Print reports
            </Button>
          </div>
        </div>

        {summary.learners === 0 && (
          <Alert tone="info" title="No learners yet">
            Add your classes' learners to start recording marks. You can add
            them one at a time from the Learners screen.
          </Alert>
        )}

        {session.isAdmin && backupStale && summary.learners > 0 && (
          <Alert tone="warning" title="Back up your data">
            {summary.lastBackupAt
              ? `The last backup was ${relativeTime(summary.lastBackupAt)}.`
              : "This school has never been backed up."}{" "}
            A term's marks are hard to rebuild. Take a backup from Settings, and
            keep one drive off site.
          </Alert>
        )}

        <div className="grid grid-stats">
          <StatTile
            label="Learners"
            value={summary.learners}
            meta={`${summary.boys} boys · ${summary.girls} girls`}
          />
          <StatTile
            label="Classes"
            value={summary.classes}
            meta={`${summary.subjects} subjects in the catalogue`}
          />
          <StatTile
            label="Teachers"
            value={summary.teachers}
            meta={summary.teachers === 0 ? "None added yet" : "Active accounts"}
          />
          <StatTile
            label="Marks this term"
            value={`${marksPercent}%`}
            meta={
              <>
                <Progress value={marksPercent} />
                <span style={{ display: "block", marginTop: 6 }}>
                  {summary.marksEnteredThisTerm.toLocaleString()} of{" "}
                  {summary.marksExpectedThisTerm.toLocaleString()} entered
                </span>
              </>
            }
          />
        </div>

        <div className="grid grid-2">
          {session.isAdmin && (
            <Card
              title="Recent activity"
              subtitle="Every change is recorded, by whom and when"
              actions={
                <Button size="sm" variant="ghost" onClick={() => navigate("audit")}>
                  View all
                </Button>
              }
              flush
            >
              {summary.recentActivity.length === 0 ? (
                <EmptyState
                  icon={<TriangleAlert size={18} />}
                  title="Nothing recorded yet"
                >
                  Activity appears here as soon as people start using RM.
                </EmptyState>
              ) : (
                <div className="table-wrap">
                  <table className="table table-compact">
                    <tbody>
                      {summary.recentActivity.map((entry) => (
                        <tr key={entry.id}>
                          <td>
                            <div>{entry.summary}</div>
                            <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                              {entry.actor_name ?? "System"} ·{" "}
                              {formatDateTime(entry.at)}
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </Card>
          )}

          <Card title="What to do next" subtitle="Suggestions based on your setup">
            <div className="stack" style={{ gap: "var(--space-3)" }}>
              <NextStep
                done={summary.teachers > 0}
                icon={<UserPlus size={15} />}
                title="Add your teachers"
                detail="Give each teacher an account, then assign them to classes and subjects."
                action={() => navigate("staff")}
                visible={session.isAdmin}
              />
              <NextStep
                done={summary.learners > 0}
                icon={<UserPlus size={15} />}
                title="Add your learners"
                detail="Register each class's learners. RM allocates registration numbers for you."
                action={() => navigate("learners")}
                visible
              />
              <NextStep
                done={summary.marksEnteredThisTerm > 0}
                icon={<ClipboardList size={15} />}
                title="Enter this term's marks"
                detail="Type straight into the grid, or use the guided form one learner at a time."
                action={() => navigate("marks")}
                visible
              />
              <NextStep
                done={Boolean(summary.lastBackupAt)}
                icon={<DatabaseBackup size={15} />}
                title="Set up a backup drive"
                detail="Point RM at an external SSD so every backup lands on it too."
                action={() => navigate("settings", { tab: "backup" })}
                visible={session.isAdmin}
              />
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}

function NextStep({
  done,
  icon,
  title,
  detail,
  action,
  visible,
}: {
  done: boolean;
  icon: React.ReactNode;
  title: string;
  detail: string;
  action: () => void;
  visible: boolean;
}) {
  if (!visible) return null;

  return (
    <button
      className="list-row"
      onClick={action}
      style={{ alignItems: "flex-start", padding: "var(--space-3)" }}
    >
      <span
        className="empty-icon"
        style={{
          width: 30,
          height: 30,
          borderRadius: "var(--radius-md)",
          background: done ? "var(--success-soft)" : "var(--accent-soft)",
          color: done ? "var(--success)" : "var(--accent)",
        }}
      >
        {icon}
      </span>
      <span className="grow" style={{ textAlign: "left" }}>
        <span className="row" style={{ gap: "var(--space-2)" }}>
          <span className="list-row-title">{title}</span>
          {done && <Badge tone="success">Done</Badge>}
        </span>
        <span className="list-row-meta" style={{ display: "block", marginTop: 2 }}>
          {detail}
        </span>
      </span>
    </button>
  );
}

/** The part of a name a greeting should use, tolerating a missing one. */
function firstName(name: string | null | undefined): string {
  if (typeof name !== "string") return "there";
  return name.trim().split(/\s+/)[0] || "there";
}

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "morning";
  if (hour < 17) return "afternoon";
  return "evening";
}
