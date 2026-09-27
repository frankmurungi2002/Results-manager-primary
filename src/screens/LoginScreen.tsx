/** FR-B1 — one login frame for every role, routed by the backend. */

import { type FormEvent, useState } from "react";
import { HardDrive, Printer, ShieldCheck, WifiOff } from "lucide-react";

import { ApiError } from "../lib/api";
import { useStore } from "../state/store";
import { SealMark } from "../components/Logo";
import { Alert, Button, TextInput } from "../components/ui";

export function LoginScreen() {
  const signIn = useStore((state) => state.signIn);
  const institutionName = useStore((state) => state.institution?.name);
  const appVersion = useStore((state) => state.appVersion);

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await signIn(username, password);
    } catch (caught) {
      // FR-B1: the message says access was denied without saying which half
      // was wrong, so a wrong username cannot be told from a wrong password.
      setError(
        caught instanceof ApiError
          ? caught.message
          : "Sign-in failed. Please try again.",
      );
      setPassword("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="auth-screen">
      <aside className="auth-aside">
        <div className="auth-wordmark">
          <span className="auth-wordmark-mark">
            <SealMark size={21} />
          </span>
          Results Manager
        </div>

        <div>
          <h1 className="auth-pitch">
            Marks in.
            <br />
            Reports out.
            <br />
            <em>Offline.</em>
          </h1>
          <p className="auth-sub">
            Every register, mark sheet and report card your school needs —
            without depending on an internet connection.
          </p>

          <div className="auth-points">
            <div className="auth-point">
              <WifiOff size={16} />
              Works with no internet, all day, every day
            </div>
            <div className="auth-point">
              <Printer size={16} />
              Report cards, class lists and registers, ready to print
            </div>
            <div className="auth-point">
              <HardDrive size={16} />
              Backed up to a second drive on a schedule
            </div>
            <div className="auth-point">
              <ShieldCheck size={16} />
              Every change recorded, nothing ever deleted
            </div>
          </div>
        </div>

        <div className="auth-foot">Version {appVersion}</div>
      </aside>

      <main className="auth-panel">
        <form className="auth-form" onSubmit={onSubmit}>
          <div>
            <h2 className="auth-title">Sign in</h2>
            <p className="auth-note">
              {institutionName
                ? `Welcome back to ${institutionName}.`
                : "Use the credentials your School Admin gave you."}
            </p>
          </div>

          {error && <Alert tone="danger">{error}</Alert>}

          <TextInput
            label="Username"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            autoComplete="username"
            autoFocus
            required
            spellCheck={false}
          />

          <TextInput
            label="Password"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            autoComplete="current-password"
            required
          />

          <Button
            type="submit"
            variant="primary"
            size="lg"
            block
            loading={busy}
            disabled={!username.trim() || !password}
          >
            Sign in
          </Button>

          <p className="field-hint" style={{ textAlign: "center" }}>
            Forgotten your password? A School Admin can reset it for you from
            the Staff screen.
          </p>
        </form>
      </main>
    </div>
  );
}
