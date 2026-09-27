/** FR-B3 — the academic calendar: years, terms and the examinations in each. */

import { useCallback, useEffect, useState } from "react";
import { CalendarDays, LockKeyhole, Unlock } from "lucide-react";

import { api } from "../lib/api";
import type { AcademicYearRow } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Loading,
  cx,
} from "../components/ui";

export function CalendarScreen() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [years, setYears] = useState<AcademicYearRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyTerm, setBusyTerm] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setYears(await api.listAcademicYears());
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  if (loading) {
    return (
      <div className="page">
        <Loading />
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Terms &amp; exams</h1>
            <p className="page-description">
              Exactly one term is open at a time, so there is never any question
              about where a mark is being filed.
            </p>
          </div>
        </div>

        {years.length === 0 ? (
          <Card>
            <EmptyState icon={<CalendarDays size={18} />} title="No academic year yet" />
          </Card>
        ) : (
          years.map((year) => (
            <Card
              key={year.id}
              title={year.label}
              subtitle={`${year.terms.length} terms`}
              actions={
                <Badge
                  tone={
                    year.status === "active"
                      ? "success"
                      : year.status === "sealed"
                        ? "info"
                        : "neutral"
                  }
                >
                  {year.status}
                </Badge>
              }
              flush
            >
              <div style={{ padding: "var(--space-2)" }}>
                {year.terms.map((term) => (
                  <div
                    key={term.id}
                    className={cx("list-row", term.status === "open" && "is-active")}
                    style={{
                      alignItems: "flex-start",
                      padding: "var(--space-3) var(--space-4)",
                      cursor: "default",
                    }}
                  >
                    <div className="grow">
                      <div className="row" style={{ gap: "var(--space-2)" }}>
                        <span className="list-row-title">{term.name}</span>
                        <Badge
                          tone={
                            term.status === "open"
                              ? "success"
                              : term.status === "closed"
                                ? "neutral"
                                : "info"
                          }
                        >
                          {term.status === "planning" ? "not started" : term.status}
                        </Badge>
                      </div>

                      <div
                        className="row"
                        style={{
                          gap: "var(--space-2)",
                          marginTop: "var(--space-2)",
                          flexWrap: "wrap",
                        }}
                      >
                        {term.exams.map((exam) => (
                          <span
                            key={exam.id}
                            className="badge badge-neutral"
                            title={`Weight ${exam.weight}`}
                          >
                            {exam.name}
                            {exam.isFinal ? " · end of term" : ""}
                            {exam.weight > 0 ? ` · ${Math.round(exam.weight * 100)}%` : ""}
                          </span>
                        ))}
                      </div>
                    </div>

                    {term.status === "open" ? (
                      <Button
                        size="sm"
                        icon={<LockKeyhole size={14} />}
                        loading={busyTerm === term.id}
                        onClick={() => {
                          if (
                            !window.confirm(
                              `Close ${term.name}? Marks can still be read and reprinted, but not changed.`,
                            )
                          )
                            return;
                          setBusyTerm(term.id);
                          api
                            .closeTerm(term.id)
                            .then(() => {
                              toast("success", `${term.name} closed.`);
                              void reload();
                            })
                            .catch(reportError)
                            .finally(() => setBusyTerm(null));
                        }}
                      >
                        Close term
                      </Button>
                    ) : (
                      <Button
                        size="sm"
                        variant="primary"
                        icon={<Unlock size={14} />}
                        loading={busyTerm === term.id}
                        onClick={() => {
                          setBusyTerm(term.id);
                          api
                            .openTerm(term.id)
                            .then(() => {
                              toast("success", `${term.name} is now open.`);
                              void reload();
                            })
                            .catch(reportError)
                            .finally(() => setBusyTerm(null));
                        }}
                      >
                        Open for marks
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            </Card>
          ))
        )}

        <Alert tone="info" title="Reopening a closed term">
          Opening a term closes whichever one was open. Everything already
          recorded stays exactly as it was — reopening only allows new edits,
          and each one is recorded against the person who made it.
        </Alert>
      </div>
    </div>
  );
}
