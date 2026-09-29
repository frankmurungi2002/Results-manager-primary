import { useEffect } from "react";

import { AppShell, ToastRegion } from "./components/AppShell";
import { Alert, Button, Loading } from "./components/ui";
import { AuditScreen } from "./screens/AuditScreen";
import { CalendarScreen } from "./screens/CalendarScreen";
import { ClassesScreen } from "./screens/ClassesScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { GradingScreen } from "./screens/GradingScreen";
import { LearnersScreen } from "./screens/LearnersScreen";
import { LoginScreen } from "./screens/LoginScreen";
import { MarksScreen } from "./screens/MarksScreen";
import { PassOutsScreen } from "./screens/PassOutsScreen";
import { WeeklyScreen } from "./screens/WeeklyScreen";
import { AttendanceScreen } from "./screens/AttendanceScreen";
import { TimetablesScreen } from "./screens/TimetablesScreen";
import { ReportsScreen } from "./screens/ReportsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { SetupWizard } from "./screens/SetupWizard";
import { StaffScreen } from "./screens/StaffScreen";
import { useStore } from "./state/store";

export default function App() {
  const booted = useStore((state) => state.booted);
  const bootError = useStore((state) => state.bootError);
  const provisioned = useStore((state) => state.provisioned);
  const session = useStore((state) => state.session);
  const screen = useStore((state) => state.screen);
  const boot = useStore((state) => state.boot);
  const refreshSession = useStore((state) => state.refreshSession);

  useEffect(() => {
    void boot();
  }, [boot]);

  // The backend expires an idle session; the window needs to notice so a
  // teacher does not stare at a dashboard they are no longer signed in to.
  useEffect(() => {
    if (!session) return;
    const handle = window.setInterval(() => void refreshSession(), 60_000);
    return () => window.clearInterval(handle);
  }, [session, refreshSession]);

  if (!booted) {
    return (
      <div className="auth-panel">
        <Loading label="Starting Phantom School Manager" />
      </div>
    );
  }

  if (bootError) {
    return (
      <div className="auth-panel">
        <div className="auth-form">
          <Alert tone="danger" title="Phantom School Manager could not start">
            {bootError}
          </Alert>
          <Button variant="primary" block onClick={() => window.location.reload()}>
            Try again
          </Button>
          <p className="field-hint">
            If this keeps happening, your data is still safe on disk. Take a
            copy of the Phantom School Manager data folder before reinstalling.
          </p>
        </div>
        <ToastRegion />
      </div>
    );
  }

  if (!provisioned) {
    return (
      <>
        <SetupWizard />
        <ToastRegion />
      </>
    );
  }

  if (!session) {
    return (
      <>
        <LoginScreen />
        <ToastRegion />
      </>
    );
  }

  return <AppShell>{renderScreen(screen)}</AppShell>;
}

function renderScreen(screen: string) {
  switch (screen) {
    case "marks":
      return <MarksScreen />;
    case "classes":
      return <ClassesScreen />;
    case "learners":
      return <LearnersScreen />;
    case "attendance":
      return <AttendanceScreen />;
    case "timetables":
      return <TimetablesScreen />;
    case "weekly":
      return <WeeklyScreen />;
    case "passouts":
      return <PassOutsScreen />;
    case "reports":
      return <ReportsScreen />;
    case "staff":
      return <StaffScreen />;
    case "calendar":
      return <CalendarScreen />;
    case "grading":
      return <GradingScreen />;
    case "audit":
      return <AuditScreen />;
    case "settings":
      return <SettingsScreen />;
    default:
      return <DashboardScreen />;
  }
}
