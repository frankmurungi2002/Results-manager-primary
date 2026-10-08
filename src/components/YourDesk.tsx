/**
 * The top of the dashboard, different for each person: the two things they
 * come to the system to do, and the few figures their job turns on. A
 * headteacher sees the school; a bursar sees fees; a Class Teacher sees their
 * own registers. Everything here comes from `dashboard_summary`, which only
 * fills a figure for someone allowed to see it.
 */

import type { ReactNode } from "react";
import {
  CalendarCheck,
  ClipboardList,
  DoorOpen,
  FileText,
  MessageSquare,
  Receipt,
  Table2,
  UserPlus,
} from "lucide-react";

import type { DashboardSummary, Permission, SessionView } from "../lib/types";
import { OFFICE_TITLES } from "../lib/types";
import type { ScreenId } from "../state/store";
import { Button, Progress } from "./ui";

interface Action {
  label: string;
  icon: ReactNode;
  screen: ScreenId;
  params?: Record<string, string>;
}

interface Figure {
  label: string;
  value: string;
  meta?: string;
  meter?: number;
  go?: { screen: ScreenId; params?: Record<string, string> };
  tone?: "warn";
}

const percent = (part: number, whole: number) => (whole > 0 ? Math.round((part / whole) * 100) : 0);

/** The two actions this person comes here for, most frequent first. */
export function deskActions(session: SessionView, can: (p: Permission) => boolean): Action[] {
  const runsAClass = session.classTeacherOf.length > 0;
  switch (session.office) {
    case "head_teacher":
      return [
        { label: "Messages", icon: <MessageSquare size={15} />, screen: "messages" },
        { label: "Print reports", icon: <FileText size={15} />, screen: "reports" },
      ];
    case "director_of_studies":
      return [
        { label: "Timetables", icon: <Table2 size={15} />, screen: "timetables" },
        { label: "Marks", icon: <ClipboardList size={15} />, screen: "marks" },
      ];
    case "secretary":
      return [
        { label: "Add learner", icon: <UserPlus size={15} />, screen: "learners" },
        { label: "Issue pass-out", icon: <DoorOpen size={15} />, screen: "passouts" },
      ];
    case "bursar":
      return [
        { label: "Text parents", icon: <MessageSquare size={15} />, screen: "messages" },
        { label: "Fees block", icon: <Receipt size={15} />, screen: "learners" },
      ];
    default:
      break;
  }
  if (runsAClass) {
    return [
      { label: "Enter marks", icon: <ClipboardList size={15} />, screen: "marks" },
      { label: "Take the register", icon: <CalendarCheck size={15} />, screen: "attendance" },
    ];
  }
  if (session.classIds.length > 0 || can("view_all_classes")) {
    return [
      { label: "Enter marks", icon: <ClipboardList size={15} />, screen: "marks" },
      { label: "Print reports", icon: <FileText size={15} />, screen: "reports" },
    ];
  }
  return [{ label: "Messages", icon: <MessageSquare size={15} />, screen: "messages" }];
}

export function YourDesk({
  session,
  summary,
  onGo,
}: {
  session: SessionView;
  summary: DashboardSummary;
  onGo: (screen: ScreenId, params?: Record<string, string>) => void;
}) {
  const { desk } = summary;
  const figures: Figure[] = [];
  const marksPercent = percent(summary.marksEnteredThisTerm, summary.marksExpectedThisTerm);

  if (desk.myRegistersPending.length > 0) {
    figures.push({
      label: "Registers to take today",
      value: desk.myRegistersPending.join(", "),
      meta: "Not yet marked",
      go: { screen: "attendance" },
      tone: "warn",
    });
  }
  if (session.office === "head_teacher" || session.office === "director_of_studies") {
    figures.push({
      label: "Marks this term",
      value: `${marksPercent}%`,
      meta: `${summary.marksEnteredThisTerm.toLocaleString()} of ${summary.marksExpectedThisTerm.toLocaleString()}`,
      meter: marksPercent,
      go: { screen: "marks" },
    });
  }
  if (desk.registersToday) {
    const { taken, classes } = desk.registersToday;
    figures.push({
      label: "Registers taken today",
      value: `${taken} of ${classes}`,
      meta: taken < classes ? `${classes - taken} class${classes - taken === 1 ? "" : "es"} still to mark` : "Every class marked",
      meter: percent(taken, classes),
      go: { screen: "attendance", params: { tab: "school" } },
      tone: taken < classes ? "warn" : undefined,
    });
  }
  if (session.office === "secretary" || session.office === "head_teacher") {
    figures.push({
      label: "Out on a pass-out",
      value: String(summary.passOutsOut),
      meta: summary.overduePassOuts.length > 0 ? `${summary.overduePassOuts.length} late back` : "None late",
      go: { screen: "passouts" },
      tone: summary.overduePassOuts.length > 0 ? "warn" : undefined,
    });
  }
  if (desk.feesBlocked !== null) {
    figures.push({
      label: "Fees block",
      value: String(desk.feesBlocked),
      meta: desk.feesBlocked === 1 ? "learner gets no report or permit" : "learners get no report or permit",
      go: { screen: "learners" },
    });
  }

  const title = session.office ? `${OFFICE_TITLES[session.office]}'s desk` : "Your desk";
  if (figures.length === 0 && desk.marksBehind.length === 0) return null;

  return (
    <section className="desk" aria-label={title}>
      <div className="desk-title">{title}</div>
      {figures.length > 0 && (
        <div className="desk-figures">
          {figures.map((figure) => (
            <button
              key={figure.label}
              className={figure.tone === "warn" ? "desk-figure is-warn" : "desk-figure"}
              onClick={() => figure.go && onGo(figure.go.screen, figure.go.params)}
            >
              <span className="desk-figure-label">{figure.label}</span>
              <span className="desk-figure-value">{figure.value}</span>
              {figure.meter !== undefined && <Progress value={figure.meter} />}
              {figure.meta && <span className="desk-figure-meta">{figure.meta}</span>}
            </button>
          ))}
        </div>
      )}
      {desk.marksBehind.length > 0 && (
        <div className="desk-behind">
          <div className="desk-behind-head">
            <span>Furthest behind with marks</span>
            <Button size="sm" variant="ghost" onClick={() => onGo("marks")}>
              Open marks
            </Button>
          </div>
          {desk.marksBehind.map((row) => {
            const done = percent(row.entered, row.expected);
            return (
              <div key={row.classId} className="desk-behind-row">
                <span className="desk-behind-class">{row.className}</span>
                <Progress value={done} />
                <span className="desk-behind-pct">{done}%</span>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
