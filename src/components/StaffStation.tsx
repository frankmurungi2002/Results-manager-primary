/**
 * The staff sign-in station, on the sign-in screen. A teacher finds their
 * name, types their PIN, and is signed in (or, the second time, out). Shows
 * names and nothing else, so it can stay open on the office desk all morning.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { CheckCircle2, Clock3, LogOut } from "lucide-react";

import { ApiError, api } from "../lib/api";
import type { StationPerson, StationResult } from "../lib/types";
import { Alert, Button, cx } from "./ui";

export function StaffStation() {
  const [people, setPeople] = useState<StationPerson[]>([]);
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<StationPerson | null>(null);
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<StationResult | null>(null);

  const load = useCallback(() => {
    api.stationStaff().then(setPeople).catch(() => setPeople([]));
  }, []);
  useEffect(load, [load]);

  // The station resets itself for the next person.
  useEffect(() => {
    if (!done) return;
    const timer = window.setTimeout(() => {
      setDone(null);
      setChosen(null);
      setQuery("");
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [done]);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    return people.filter((p) => !q || p.fullName.toLowerCase().includes(q)).slice(0, 8);
  }, [people, query]);
  const inNow = people.filter((p) => p.signedInAt && !p.signedOutAt).length;

  async function sign() {
    if (!chosen) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.stationSign(chosen.userId, pin);
      setDone(result);
      setPin("");
      load();
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.message : "That did not work. Try again.");
      setPin("");
    } finally {
      setBusy(false);
    }
  }

  if (done) {
    return (
      <div className="station-done">
        {done.action === "in" ? <CheckCircle2 size={36} /> : <LogOut size={36} />}
        <strong>
          {done.action === "in" ? "Good morning" : "Goodbye"}, {done.fullName.split(" ")[0]}
        </strong>
        <span>
          Signed {done.action} at {done.time}
          {done.action === "in" && done.late ? " (after the start time)" : ""}
        </span>
      </div>
    );
  }

  return (
    <div className="station">
      {error && <Alert tone="danger">{error}</Alert>}
      {!chosen ? (
        <>
          <input
            className="login-input"
            placeholder="Type your name"
            value={query}
            autoFocus
            onChange={(e) => setQuery(e.target.value)}
          />
          <div className="station-list">
            {matches.map((p) => (
              <button
                key={p.userId}
                type="button"
                className={cx("station-person", p.signedInAt && "is-in")}
                onClick={() => {
                  setChosen(p);
                  setError(null);
                }}
              >
                <span>{p.fullName}</span>
                <span className="station-state">
                  {p.signedOutAt ? `Out ${p.signedOutAt}` : p.signedInAt ? `In ${p.signedInAt}` : ""}
                </span>
              </button>
            ))}
          </div>
          <p className="login-help">
            <Clock3 size={12} /> {inNow} in school now
          </p>
        </>
      ) : (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void sign();
          }}
        >
          <div className="station-chosen">
            <strong>{chosen.fullName}</strong>
            <button type="button" className="login-link" onClick={() => setChosen(null)}>
              Not you?
            </button>
          </div>
          {!chosen.hasPin ? (
            <p className="login-help">
              You have no PIN yet. Sign in to Phantom once and set it under My account, or ask the office to record
              you.
            </p>
          ) : (
            <>
              <input
                className="login-input"
                type="password"
                inputMode="numeric"
                autoFocus
                placeholder="Your PIN"
                value={pin}
                maxLength={6}
                onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))}
              />
              <Button type="submit" variant="primary" block loading={busy} disabled={pin.length < 4} className="login-submit">
                {chosen.signedInAt ? "Sign out" : "Sign in"}
              </Button>
            </>
          )}
        </form>
      )}
    </div>
  );
}
