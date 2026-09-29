/** Sidebar, top bar and toast region — the frame every signed-in screen sits in. */

import { useEffect, useState } from "react";
import {
  BookOpenCheck,
  CalendarDays,
  ChevronDown,
  ClipboardList,
  FileText,
  GraduationCap,
  LayoutDashboard,
  LogOut,
  Moon,
  ScrollText,
  Settings,
  Shield,
  Sun,
  Users,
  UsersRound,
  X,
} from "lucide-react";

import { api } from "../lib/api";
import { type ScreenId, useStore } from "../state/store";
import { SealMark } from "./Logo";
import { Badge, Button, cx, initials } from "./ui";

interface NavEntry {
  id: ScreenId;
  label: string;
  icon: typeof LayoutDashboard;
  adminOnly?: boolean;
}

const NAV_SECTIONS: { heading: string; items: NavEntry[] }[] = [
  {
    heading: "Daily work",
    items: [
      { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
      { id: "marks", label: "Marks entry", icon: ClipboardList },
      { id: "learners", label: "Learners", icon: Users },
      { id: "reports", label: "Reports & printing", icon: FileText },
    ],
  },
  {
    heading: "Set up the school",
    items: [
      { id: "classes", label: "Classes & subjects", icon: BookOpenCheck },
      { id: "calendar", label: "Terms & exams", icon: CalendarDays, adminOnly: true },
      { id: "grading", label: "Grading systems", icon: GraduationCap, adminOnly: true },
      { id: "staff", label: "Staff", icon: UsersRound, adminOnly: true },
    ],
  },
  {
    heading: "School office",
    items: [
      { id: "audit", label: "Activity log", icon: ScrollText, adminOnly: true },
      { id: "settings", label: "Settings", icon: Settings },
    ],
  },
];

const SCREEN_TITLES: Record<ScreenId, string> = {
  dashboard: "Dashboard",
  marks: "Marks entry",
  classes: "Classes & subjects",
  learners: "Learners",
  reports: "Reports & printing",
  staff: "Staff",
  calendar: "Terms & exams",
  grading: "Grading systems",
  audit: "Activity log",
  settings: "Settings",
};

export function AppShell({ children }: { children: React.ReactNode }) {
  const session = useStore((state) => state.session);
  const institution = useStore((state) => state.institution);
  const screen = useStore((state) => state.screen);
  const navigate = useStore((state) => state.navigate);
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
  }, [institution?.hasLogo]);

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

  const roleLabel =
    session.role === "school_admin"
      ? "School Admin"
      : session.mode === "class_teacher"
        ? "Class Teacher"
        : "Subject Teacher";

  return (
    <div className="app">
      <div className="app-card">
      <nav className="sidebar">
        <div className="sidebar-brand">
          <div className="sidebar-mark">
            {logo ? <img src={logo} alt="" /> : <SealMark size={18} />}
          </div>
          <div className="sidebar-brand-text">
            <div className="sidebar-brand-name">
              {institution?.name ?? "Results Manager"}
            </div>
            <div className="sidebar-brand-meta">Results Manager</div>
          </div>
        </div>

        <div className="sidebar-nav">
          {NAV_SECTIONS.map((section) => {
            const visible = section.items.filter(
              (item) => !item.adminOnly || session.isAdmin,
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
          <div className="sidebar-note" title={`Results Manager version ${appVersion}`}>
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
                Results Manager {appVersion}
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
