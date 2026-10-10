/** Sidebar, top bar and toast region — the frame every signed-in screen sits in. */

import { useEffect, useState } from "react";
import {
  Award,
  Wallet,
  ChartColumnBig,
  Banknote,
  BookOpenCheck,
  BookText,
  CalendarCheck,
  CalendarDays,
  ChevronDown,
  ClipboardCheck,
  ClipboardList,
  FileText,
  GraduationCap,
  DoorOpen,
  LayoutDashboard,
  LogOut,
  MessageSquare,
  NotebookPen,
  Moon,
  ScanLine,
  ScrollText,
  Settings,
  Shield,
  Sun,
  Table2,
  Users,
  UsersRound,
  X,
} from "lucide-react";

import { api } from "../lib/api";
import { OFFICE_TITLES } from "../lib/types";
import type { FeatureFlags, Permission, SessionView } from "../lib/types";
import { type ScreenId, useCan, useStore } from "../state/store";
import { APP_NAME, BrandMark } from "./Logo";
import { LicenceBanner, useLicence } from "./Subscription";
import { Badge, Button, cx, initials } from "./ui";

interface NavEntry {
  id: ScreenId;
  label: string;
  icon: typeof LayoutDashboard;
  /**
   * Who sees it, from their permissions and their own classes. Left out, it
   * is for everyone. The backend decides access again on every command; this
   * only keeps people from being offered screens that would refuse them.
   */
  show?: (can: (p: Permission) => boolean, session: SessionView) => boolean;
  /** Shown only while this optional feature (FR-B7) is switched on. */
  feature?: keyof FeatureFlags;
}

const teaches = (session: SessionView) => session.classIds.length > 0;
const runsAClass = (session: SessionView) => session.classTeacherOf.length > 0;

const NAV_SECTIONS: { heading: string; items: NavEntry[] }[] = [
  {
    heading: "Daily work",
    items: [
      { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
      { id: "marks", label: "Marks entry", icon: ClipboardList, show: (can, s) => teaches(s) || can("view_all_classes") },
      {
        id: "weekly",
        label: "Weekly assignments",
        icon: NotebookPen,
        feature: "weeklyAssignments",
        show: (can, s) => teaches(s) || can("view_all_classes"),
      },
      { id: "attendance", label: "Attendance", icon: CalendarCheck, show: (can, s) => teaches(s) || can("view_all_classes") },
      { id: "learners", label: "Learners", icon: Users, show: (can, s) => teaches(s) || can("view_all_learners") },
      {
        id: "insights",
        label: "Insights",
        icon: ChartColumnBig,
        show: (can, s) => can("view_insights") || s.classIds.length > 0,
      },
      { id: "requirements", label: "Requirements", icon: ClipboardCheck, show: (can, s) => runsAClass(s) || can("view_all_learners") },
      { id: "passouts", label: "Pass-outs", icon: DoorOpen, show: (can, s) => runsAClass(s) || can("issue_pass_outs") },
      { id: "messages", label: "Messages", icon: MessageSquare },
      { id: "reports", label: "Reports & printing", icon: FileText, show: (can, s) => teaches(s) || can("view_all_learners") },
    ],
  },
  {
    heading: "Set up the school",
    items: [
      { id: "classes", label: "Classes & subjects", icon: BookOpenCheck, show: (can, s) => teaches(s) || can("manage_academics") || can("view_all_classes") },
      { id: "calendar", label: "Terms & exams", icon: CalendarDays, show: (can) => can("manage_calendar") },
      { id: "timetables", label: "Timetables", icon: Table2 },
      { id: "grading", label: "Grading systems", icon: GraduationCap, show: (can) => can("manage_school") },
      { id: "staff", label: "Staff", icon: UsersRound, show: (can) => can("manage_staff") },
    ],
  },
  {
    heading: "School office",
    items: [
      { id: "daybook", label: "Day Book", icon: BookText, show: (can) => can("view_finance") || can("view_all_classes") },
      { id: "fees", label: "Fees", icon: Banknote, show: (can) => can("view_finance") || can("manage_fees") },
      { id: "expenses", label: "Expenses", icon: Wallet, show: (can) => can("view_finance") || can("manage_fees") },
      { id: "verify", label: "Check a document", icon: ScanLine },
      {
        id: "registrations",
        label: "Registrations",
        icon: Award,
        show: (can) => can("view_all_learners") || can("manage_academics") || can("manage_all_classes"),
      },
      { id: "audit", label: "Activity log", icon: ScrollText, show: (can) => can("view_audit") },
      { id: "settings", label: "Settings", icon: Settings },
    ],
  },
];

const SCREEN_TITLES: Record<ScreenId, string> = {
  dashboard: "Dashboard",
  attendance: "Attendance",
  timetables: "Timetables",
  weekly: "Weekly assignments",
  passouts: "Pass-outs",
  messages: "Messages",
  marks: "Marks entry",
  classes: "Classes & subjects",
  learners: "Learners",
  reports: "Reports & printing",
  staff: "Staff",
  calendar: "Terms & exams",
  grading: "Grading systems",
  audit: "Activity log",
  fees: "Fees",
  verify: "Check a document",
  daybook: "Day Book",
  requirements: "Requirements",
  registrations: "Registrations: PLE, EMIS and sports",
  insights: "Insights",
  expenses: "Expenses and cash",
  settings: "Settings",
};

export function AppShell({ children }: { children: React.ReactNode }) {
  const session = useStore((state) => state.session);
  const can = useCan();
  const [licence] = useLicence();
  const institution = useStore((state) => state.institution);
  const screen = useStore((state) => state.screen);
  const navigate = useStore((state) => state.navigate);
  const features = useStore((state) => state.features);
  const signOut = useStore((state) => state.signOut);
  const theme = useStore((state) => state.theme);
  const setTheme = useStore((state) => state.setTheme);
  const appVersion = useStore((state) => state.appVersion);

  const [logo, setLogo] = useState<string | null>(null);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [term, setTerm] = useState<{ year: string | null; term: string | null }>({
    year: null,
    term: null,
  });

  // Re-read on every refresh of the institution, so a replaced logo shows at
  // once rather than after a restart.
  useEffect(() => {
    if (!institution?.hasLogo) {
      setLogo(null);
      return;
    }
    let cancelled = false;
    api
      .getInstitutionLogo()
      .then((bytes) => {
        if (cancelled || !bytes) return;
        const blob = new Blob([new Uint8Array(bytes)], { type: "image/png" });
        setLogo(URL.createObjectURL(blob));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [institution]);

  useEffect(() => {
    api
      .dashboardSummary()
      .then((summary) =>
        setTerm({ year: summary.academicYear, term: summary.currentTerm }),
      )
      .catch(() => undefined);
  }, [screen]);

  if (!session) return null;

  // A name is the one thing the shell cannot do without. If it is missing the
  // session shape is wrong, which is a developer fault, not a user one — say so
  // plainly instead of drawing a convincing but broken chrome.
  const displayName = session.fullName ?? session.username ?? "Unknown user";

  // An office names the person first: a headteacher who teaches P7 is still
  // "Headteacher" in the corner of the screen.
  const roleLabel = session.office
    ? OFFICE_TITLES[session.office]
    : session.role === "school_admin"
      ? "School Admin"
      : session.mode === "class_teacher"
        ? "Class Teacher"
        : "Subject Teacher";

  return (
    <div className="app">
      <div className="app-card">
      <nav className="sidebar">
        <div className="sidebar-brand">
          {/* The school's own logo. Until one is uploaded, its initials. */}
          {logo ? (
            <div className="sidebar-mark">
              <img src={logo} alt="" />
            </div>
          ) : can("manage_school") ? (
            <button
              className="sidebar-mark sidebar-mark-empty"
              title="Add your school's logo"
              onClick={() => navigate("settings", { tab: "school" })}
            >
              {initials(institution?.name ?? "School")}
            </button>
          ) : (
            <div className="sidebar-mark sidebar-mark-empty">
              {initials(institution?.name ?? "School")}
            </div>
          )}
          <div className="sidebar-brand-text">
            <div className="sidebar-brand-name">
              {institution?.name ?? APP_NAME}
            </div>
            <div className="sidebar-brand-meta">
              <BrandMark size={12} />
              {APP_NAME}
            </div>
          </div>
        </div>

        <div className="sidebar-nav">
          {NAV_SECTIONS.map((section) => {
            const visible = section.items.filter(
              (item) =>
                (!item.show || item.show(can, session)) &&
                (!item.feature || features[item.feature]),
            );
            if (visible.length === 0) return null;

            return (
              <div key={section.heading}>
                <div className="nav-section">{section.heading}</div>
                {visible.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      className={cx("nav-item", screen === item.id && "is-active")}
                      onClick={() => navigate(item.id)}
                      title={item.label}
                    >
                      <Icon size={16} />
                      <span className="nav-label">{item.label}</span>
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>

        <div className="sidebar-foot">
          <div className="sidebar-note" title={`${APP_NAME} version ${appVersion}`}>
            <span className="sidebar-version-full">Version {appVersion}</span>
            <span className="sidebar-version-short">v{appVersion}</span>
          </div>
        </div>
      </nav>

      <div className="main">
        <header className="topbar">
          <div>
            <div className="topbar-title">{SCREEN_TITLES[screen]}</div>
            {institution && (
              <div className="topbar-crumb">{institution.name}</div>
            )}
          </div>

          <div className="topbar-actions">
            {term.term && (
              <span className="term-pill">
                <CalendarDays size={13} />
                {term.term}
                {term.year ? ` · ${term.year}` : ""}
              </span>
            )}

            {/* FR-C2: switch between the two teacher views. */}
            {session.role === "teacher" && session.canSwitchToClassTeacher && (
              <TeacherModeSwitch />
            )}

            <button
              type="button"
              className="topbar-icon"
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
              title={theme === "dark" ? "Switch to light" : "Switch to dark"}
              aria-label={theme === "dark" ? "Switch to light" : "Switch to dark"}
            >
              {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
            </button>

            <button type="button" className="topbar-user" onClick={() => setUserMenuOpen(true)}>
              <span className="avatar">{initials(displayName)}</span>
              <span className="topbar-user-text">
                <span className="topbar-user-name">{displayName}</span>
                <span className="topbar-user-role">{roleLabel}</span>
              </span>
              <ChevronDown size={14} className="subtle" />
            </button>
          </div>
        </header>

        <LicenceBanner status={licence} />
        {children}
      </div>
      </div>

      {userMenuOpen && (
        <div
          className="modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setUserMenuOpen(false);
          }}
        >
          <div className="modal" style={{ maxWidth: 380 }}>
            <div className="modal-header">
              <div className="row">
                <span className="avatar" style={{ width: 38, height: 38 }}>
                  {initials(displayName)}
                </span>
                <div>
                  <div style={{ fontWeight: 600 }}>{displayName}</div>
                  <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
                    {session.username} · {roleLabel}
                  </div>
                </div>
              </div>
              <Button
                variant="ghost"
                size="sm"
                icon={<X size={16} />}
                onClick={() => setUserMenuOpen(false)}
              />
            </div>
            <div className="modal-body stack">
              {session.mustChangePassword && (
                <Badge tone="warning">Password change required</Badge>
              )}
              <Button
                icon={<Shield size={15} />}
                block
                onClick={() => {
                  setUserMenuOpen(false);
                  navigate("settings", { tab: "account" });
                }}
              >
                Change my password
              </Button>
              <Button
                variant="danger"
                icon={<LogOut size={15} />}
                block
                onClick={() => {
                  setUserMenuOpen(false);
                  void signOut();
                }}
              >
                Sign out
              </Button>
              <p className="subtle" style={{ fontSize: "var(--text-xs)", textAlign: "center" }}>
                {APP_NAME} {appVersion}
              </p>
            </div>
          </div>
        </div>
      )}

      <ToastRegion />
    </div>
  );
}

function TeacherModeSwitch() {
  const session = useStore((state) => state.session);
  const setTeacherMode = useStore((state) => state.setTeacherMode);
  const reportError = useStore((state) => state.reportError);

  if (!session) return null;

  return (
    <div className="segmented">
      <button
        className={cx(session.mode === "class_teacher" && "is-active")}
        onClick={() => setTeacherMode("class_teacher").catch(reportError)}
      >
        Class Teacher
      </button>
      <button
        className={cx(session.mode === "subject_teacher" && "is-active")}
        onClick={() => setTeacherMode("subject_teacher").catch(reportError)}
      >
        Subject Teacher
      </button>
    </div>
  );
}

export function ToastRegion() {
  const toasts = useStore((state) => state.toasts);
  const dismiss = useStore((state) => state.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-region no-print">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={cx(
            "toast",
            toast.kind === "success" && "toast-success",
            toast.kind === "error" && "toast-error",
            toast.kind === "warning" && "toast-warning",
          )}
          role="status"
        >
          <span className="grow">{toast.message}</span>
          <button
            onClick={() => dismiss(toast.id)}
            aria-label="Dismiss"
            className="subtle"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
