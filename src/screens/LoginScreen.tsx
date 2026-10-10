/** FR-B1 — one login frame for every role, routed by the backend. */

import { type FormEvent, useEffect, useState } from "react";
import { Eye, EyeOff } from "lucide-react";

import { ApiError, api } from "../lib/api";
import { useStore } from "../state/store";
import { APP_NAME, BrandMark } from "../components/Logo";
import { PRACTICE_LOGINS, PRACTICE_PASSWORD } from "../lib/practice";
import { LoginShowcase } from "../components/LoginShowcase";
import { StaffStation } from "../components/StaffStation";
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
  const [tab, setTab] = useState<"password" | "station" | "recover" | "invite">("password");
  const [stationOn, setStationOn] = useState(false);
  const [practice, setPractice] = useState(false);
  useEffect(() => {
    api.practiceStatus().then((p) => setPractice(p.practice)).catch(() => undefined);
  }, []);

  // The sign-in station is offered only once the school has switched it on.
  useEffect(() => {
    api.stationStaff().then(() => setStationOn(true)).catch(() => setStationOn(false));
  }, []);

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
      <div className="login-split">
        <section className="login-left">
          <header className="login-brand">
            <span className="login-brand-mark">
              <BrandMark size={32} />
            </span>
            {APP_NAME}
          </header>

          <div className="login-center">
            {institutionName && <div className="login-school">{institutionName}</div>}

            <div className="login-panel">
              <h2 className="login-title">{tab === "station" ? "Arriving or leaving?" : tab === "recover" ? "Recovery code" : tab === "invite" ? "Set your password" : "Welcome back."}</h2>

              <div className="login-tabs" role="tablist">
                <button type="button" role="tab" className={tab === "password" ? "login-tab is-active" : "login-tab"} onClick={() => setTab("password")}>
                  Username &amp; password
                </button>
                {stationOn && (
                  <button type="button" role="tab" className={tab === "station" ? "login-tab is-active" : "login-tab"} onClick={() => setTab("station")}>
                    Staff sign-in
                  </button>
                )}
              </div>

              {tab === "station" ? (
                <StaffStation />
              ) : tab === "recover" ? (
                <RecoverForm kind="recover" onDone={() => setTab("password")} />
              ) : tab === "invite" ? (
                <RecoverForm kind="invite" onDone={() => setTab("password")} />
              ) : (
              <form className="login-form" onSubmit={onSubmit} noValidate>

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
                  screen. A School Admin who has forgotten their own password:{" "}
                  <button type="button" className="login-link" onClick={() => setTab("recover")}>
                    use my recovery code
                  </button>
                  .
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
              )}
            </div>

            {practice && tab === "password" && (
              <div className="practice-hint">
                <strong>Practice school.</strong> Sign in as {PRACTICE_LOGINS.map((l) => l.username).join(", ")}; every password is{" "}
                <span className="mono">{PRACTICE_PASSWORD}</span>.
              </div>
            )}

            <p className="login-after" style={{ visibility: tab === "invite" ? "hidden" : undefined }}>
              New staff?{" "}
              <button type="button" className="login-link" onClick={() => setTab("invite")}>
                I have an invite code
              </button>
            </p>
          </div>

          <footer className="login-foot">Version {appVersion}</footer>
        </section>

        <LoginShowcase />
      </div>
    </div>
  );
}

/**
 * A one-time code sets a new password: a School Admin's printed recovery
 * code, or the invite code a new member of staff was sent.
 */
function RecoverForm({ kind, onDone }: { kind: "recover" | "invite"; onDone: () => void }) {
  const [username, setUsername] = useState("");
  const [code, setCode] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);

  if (done) {
    return (
      <div className="station">
        <Alert tone="success">
          {kind === "invite"
            ? "Your password is set. Sign in with your username and the password you just chose."
            : "Your password is set and the recovery code is spent. Sign in, then print a new recovery sheet under Settings → My account."}
        </Alert>
        <Button variant="primary" block className="login-submit" onClick={onDone}>
          Sign in
        </Button>
      </div>
    );
  }
  return (
    <form
      className="login-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (next !== again) {
          setError("The two passwords do not match.");
          return;
        }
        setBusy(true);
        setError(null);
        (kind === "invite" ? api.acceptInvite(username, code, next).then(() => undefined) : api.useRecoveryCode(username, code, next))
          .then(() => setDone(true))
          .catch((caught) => setError(caught instanceof ApiError ? caught.message : "That did not work."))
          .finally(() => setBusy(false));
      }}
    >
      {error && <Alert tone="danger">{error}</Alert>}
      <input className="login-input" placeholder="Your username" value={username} onChange={(e) => setUsername(e.target.value)} />
      {kind === "invite" && <p className="login-help">Type the username and invite code from your SMS or slip, then choose your own password.</p>}
      <input
        className="login-input"
        placeholder={kind === "invite" ? "INV-XXXX-XXXX" : "RCV-XXXX-XXXX-XXXX-XXXX"}
        value={code}
        onChange={(e) => setCode(e.target.value)}
      />
      <input className="login-input" type="password" placeholder="New password (10+ characters)" value={next} onChange={(e) => setNext(e.target.value)} />
      <input className="login-input" type="password" placeholder="New password again" value={again} onChange={(e) => setAgain(e.target.value)} />
      <Button type="submit" variant="primary" block loading={busy} disabled={!username || code.length < (kind === "invite" ? 8 : 16) || next.length < 10} className="login-submit">
        Set my new password
      </Button>
      <button type="button" className="login-link" onClick={onDone}>
        Back to sign in
      </button>
    </form>
  );
}
