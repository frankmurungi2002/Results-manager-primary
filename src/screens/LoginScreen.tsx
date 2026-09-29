/** FR-B1 — one login frame for every role, routed by the backend. */

import { type FormEvent, useState } from "react";
import { Eye, EyeOff, WifiOff } from "lucide-react";

import { ApiError } from "../lib/api";
import { useStore } from "../state/store";
import { SealMark } from "../components/Logo";
import { LoginIllustration } from "../components/LoginIllustration";
import { Alert, Button } from "../components/ui";

export function LoginScreen() {
  const signIn = useStore((state) => state.signIn);
  const institutionName = useStore((state) => state.institution?.name);
  const appVersion = useStore((state) => state.appVersion);

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
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
    <div className="login-screen">
      <div className="login-card">
        <header className="login-bar">
          <span className="login-brand">
            <span className="login-brand-mark">
              <SealMark size={17} />
            </span>
            Results Manager
          </span>
          <span className="login-bar-title">{institutionName ?? "School Results Manager"}</span>
          <span className="login-bar-end">
            <span className="login-offline" title="Everything works without an internet connection">
              <WifiOff size={13} />
              Offline
            </span>
          </span>
        </header>

        <section className="login-hero">
          <h1 className="login-headline">
            Marks in. Reports out.
            <br />
            Even without the internet.
          </h1>
          <span className="login-pill">Version {appVersion}</span>
        </section>

        <div className="login-body">
          <LoginIllustration className="login-illustration" />

          <div className="login-side">
            <form className="login-panel" onSubmit={onSubmit} noValidate>
              <h2 className="login-title">Welcome back.</h2>

              <div className="login-tabs" role="presentation">
                <span className="login-tab is-active">Username &amp; password</span>
              </div>

              {error && <Alert tone="danger">{error}</Alert>}

              <label className="login-field">
                <span className="visually-hidden">Username</span>
                <input
                  className="login-input"
                  placeholder="Username"
                  value={username}
                  onChange={(event) => setUsername(event.target.value)}
                  autoComplete="username"
                  autoFocus
                  spellCheck={false}
                  required
                />
              </label>

              <label className="login-field">
                <span className="visually-hidden">Password</span>
                <input
                  className="login-input login-input-password"
                  type={showPassword ? "text" : "password"}
                  placeholder="Password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  autoComplete="current-password"
                  required
                />
                <span className="login-field-actions">
                  <button
                    type="button"
                    className="login-eye"
                    onClick={() => setShowPassword((shown) => !shown)}
                    aria-label={showPassword ? "Hide password" : "Show password"}
                    title={showPassword ? "Hide password" : "Show password"}
                  >
                    {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                  <button
                    type="button"
                    className="login-link"
                    onClick={() => setShowHelp((open) => !open)}
                    aria-expanded={showHelp}
                  >
                    Forgot?
                  </button>
                </span>
              </label>

              {showHelp && (
                <p className="login-help">
                  A School Admin can set a new password for you from the Staff
                  screen.
                </p>
              )}

              <Button
                type="submit"
                variant="primary"
                block
                loading={busy}
                disabled={!username.trim() || !password}
                className="login-submit"
              >
                Login
              </Button>
            </form>

            <p className="login-after">New staff? Your School Admin creates your account.</p>
          </div>
        </div>
      </div>
    </div>
  );
}
