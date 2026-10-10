/**
 * Find one learner by name or registration number. Used wherever a screen
 * acts on a single learner: fees, sports, discipline, PLE checks.
 */

import { useEffect, useState } from "react";
import { Search } from "lucide-react";

import { api } from "../lib/api";
import type { StudentRow } from "../lib/types";
import { useStore } from "../state/store";
import { Button } from "./ui";

export function LearnerPicker({
  value,
  onChange,
  label = "Learner",
  autoFocus = false,
  includeLeft = false,
}: {
  value: StudentRow | null;
  onChange: (learner: StudentRow | null) => void;
  label?: string;
  autoFocus?: boolean;
  /** Also find learners who have left (a former pupil paying a balance). */
  includeLeft?: boolean;
}) {
  const reportError = useStore((state) => state.reportError);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<StudentRow[]>([]);

  useEffect(() => {
    if (value || query.trim().length < 2) {
      setMatches([]);
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .searchStudents(query.trim())
        .then((found) => setMatches(includeLeft ? found : found.filter((s) => s.enrollmentStatus === "active")))
        .catch(reportError);
    }, 200);
    return () => window.clearTimeout(timer);
  }, [query, value, includeLeft, reportError]);

  if (value) {
    return (
      <div className="po-picked">
        <div>
          <div style={{ fontWeight: 600 }}>{value.fullName}</div>
          <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
            {value.className ?? "Not in a class"} • {value.regNumber}
            {value.guardianName ? ` • Parent: ${value.guardianName}` : ""}
          </div>
        </div>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            setQuery("");
            onChange(null);
          }}
        >
          Change
        </Button>
      </div>
    );
  }

  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div className="input-icon">
        <Search size={15} />
        <input
          className="input"
          autoFocus={autoFocus}
          placeholder="Type a name or registration number"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      {matches.length > 0 && (
        <div className="po-matches">
          {matches.slice(0, 8).map((student) => (
            <button key={student.id} className="po-match" onClick={() => onChange(student)}>
              <span style={{ fontWeight: 600 }}>{student.fullName}</span>
              <span className="subtle">
                {student.className ?? "Left"} • {student.regNumber}
              </span>
            </button>
          ))}
        </div>
      )}
      {query.trim().length >= 2 && matches.length === 0 && (
        <span className="field-hint">No learner matches “{query.trim()}” yet.</span>
      )}
    </div>
  );
}
