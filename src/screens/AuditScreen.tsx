/**
 * FR-E2 — the activity log, and only here: it is not on the dashboard.
 *
 * Append-only in the database and sealed by a hash chain, so a change made
 * outside the application (with a copy of SQLite) is detected and named. A
 * printout carries a verification code that proves, later, whether anything
 * up to that point has been rewritten.
 */

import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Printer, ScrollText, Search, ShieldAlert, ShieldCheck } from "lucide-react";

import { api } from "../lib/api";
import type { AuditEntry, AuditExport, AuditIntegrity, CodeCheck, DocumentEnvelope } from "../lib/types";
import { useStore } from "../state/store";
import { AuditLogSheet, PrintPreview, ROLE_TITLES } from "../components/PrintDocument";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  TableSkeleton,
  TextInput,
  formatDateTime,
} from "../components/ui";

const PAGE_SIZE = 60;

export function AuditScreen() {
  const reportError = useStore((state) => state.reportError);

  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(true);
  const [atEnd, setAtEnd] = useState(false);
  const [integrity, setIntegrity] = useState<AuditIntegrity | null>(null);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [printing, setPrinting] = useState(false);
  const [printout, setPrintout] = useState<DocumentEnvelope<AuditExport> | null>(null);
  const [code, setCode] = useState("");
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState<CodeCheck | null>(null);

  const loadIntegrity = useCallback(() => {
    api.auditIntegrity().then(setIntegrity).catch(reportError);
  }, [reportError]);

  useEffect(loadIntegrity, [loadIntegrity]);

  const load = useCallback(
    async (searchTerm: string, nextOffset: number, append: boolean) => {
      setLoading(true);
      try {
        const page = await api.searchAuditLog(
          searchTerm.trim() || undefined,
          PAGE_SIZE,
          nextOffset,
        );
        setEntries((current) => (append ? [...current, ...page] : page));
        setAtEnd(page.length < PAGE_SIZE);
        setOffset(nextOffset);
      } catch (error) {
        reportError(error);
      } finally {
        setLoading(false);
      }
    },
    [reportError],
  );

  useEffect(() => {
    const handle = window.setTimeout(() => void load(query, 0, false), 240);
    return () => window.clearTimeout(handle);
  }, [query, load]);

  if (printout) {
    return (
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <PrintPreview
          actions={
            <>
              <Button
                variant="ghost"
                icon={<ArrowLeft size={15} />}
                onClick={() => {
                  setPrintout(null);
                  loadIntegrity();
                  void load(query, 0, false);
                }}
              >
                Back
              </Button>
              <span className="subtle" style={{ fontSize: "var(--text-sm)" }}>
                To save a file instead of paper, choose “Microsoft Print to PDF” as the printer.
              </span>
            </>
          }
        >
          <AuditLogSheet envelope={printout} />
        </PrintPreview>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Activity log</h1>
            <p className="page-description">
              Every action by every person, with a timestamp. Entries cannot be
              edited or removed, and each one is sealed to the one before it, so
              a change made outside Phantom School Manager is found and named.
            </p>
          </div>
        </div>

        {integrity &&
          (integrity.report.intact ? (
            <div className="audit-seal is-ok">
              <ShieldCheck size={20} />
              <div>
                <strong>The log is intact.</strong> All {integrity.report.entries.toLocaleString()} entries
                match their seals.
              </div>
            </div>
          ) : (
            <div className="audit-seal is-broken" role="alert">
              <ShieldAlert size={20} />
              <div>
                <strong>The log has been altered outside Phantom School Manager.</strong> {integrity.report.problem}{" "}
                Nothing the application did can cause this. Keep your printed logs and backups; they show what the
                record said before.
              </div>
            </div>
          ))}

        <div className="grid grid-2">
          <Card title="Print or save the log" subtitle="With a code that proves later it has not been rewritten">
            <div className="stack">
              <div className="row" style={{ gap: "var(--space-3)" }}>
                <TextInput label="From" type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
                <TextInput label="To" type="date" value={to} onChange={(e) => setTo(e.target.value)} />
              </div>
              <p className="field-hint">
                Leave the dates empty for everything. The search box below narrows it further.
              </p>
              <Button
                variant="primary"
                icon={<Printer size={15} />}
                loading={printing}
                onClick={() => {
                  setPrinting(true);
                  api
                    .buildAuditExport(from || null, to || null, query.trim() || null)
                    .then(setPrintout)
                    .catch(reportError)
                    .finally(() => setPrinting(false));
                }}
              >
                Prepare printout
              </Button>
            </div>
          </Card>

          <Card title="Check a printout" subtitle="Type the verification code from the bottom of a printed log">
            <div className="stack">
              <TextInput
                label="Verification code"
                placeholder="1234-K7QX-3MZT-9PRH"
                value={code}
                onChange={(e) => {
                  setCode(e.target.value);
                  setChecked(null);
                }}
                className="mono"
              />
              <Button
                loading={checking}
                disabled={code.trim().length < 8}
                onClick={() => {
                  setChecking(true);
                  api
                    .verifyAuditCode(code)
                    .then(setChecked)
                    .catch(reportError)
                    .finally(() => setChecking(false));
                }}
              >
                Check
              </Button>
              {checked && (
                <Alert tone={checked.matches ? "success" : "danger"} title={checked.matches ? "It matches" : "It does not match"}>
                  {checked.message}
                </Alert>
              )}
            </div>
          </Card>
        </div>

        <Card>
          <div className="field">
            <span className="field-label">Search</span>
            <div style={{ position: "relative" }}>
              <Search
                size={15}
                style={{
                  position: "absolute",
                  left: 10,
                  top: "50%",
                  transform: "translateY(-50%)",
                  color: "var(--text-tertiary)",
                }}
              />
              <input
                className="input"
                style={{ paddingLeft: 32 }}
                placeholder="A person's name, an action, or part of the description"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
          </div>
        </Card>

        <Card flush>
          {loading && entries.length === 0 ? (
            <TableSkeleton rows={10} columns={3} />
          ) : entries.length === 0 ? (
            <EmptyState icon={<ScrollText size={18} />} title="Nothing matches">
              Try a shorter search, or clear the box to see everything.
            </EmptyState>
          ) : (
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr>
                    <th style={{ width: 170 }}>When</th>
                    <th style={{ width: 190 }}>Who</th>
                    <th>What happened</th>
                    <th style={{ width: 170 }}>Action</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <tr
                      key={entry.id}
                      className={integrity?.report.problemEntry === entry.id ? "audit-row-broken" : undefined}
                    >
                      <td className="muted">
                        <span className="mono subtle">#{entry.id}</span> {formatDateTime(entry.at)}
                      </td>
                      <td>
                        {entry.actor_name ?? <span className="subtle">System</span>}
                        {entry.actor_role && (
                          <div style={{ marginTop: 2 }}>
                            <Badge tone={entry.actor_role === "teacher" ? "neutral" : "accent"}>
                              {ROLE_TITLES[entry.actor_role] ?? entry.actor_role}
                            </Badge>
                          </div>
                        )}
                      </td>
                      <td>{entry.summary}</td>
                      <td className="mono subtle">{entry.action}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>

        {!atEnd && entries.length > 0 && (
          <Button
            block
            loading={loading}
            onClick={() => void load(query, offset + PAGE_SIZE, true)}
          >
            Load more
          </Button>
        )}

        <Alert tone="info" title="Why nothing here can be changed">
          If the record of what happened could be edited, it would not be worth
          keeping. The database rejects every update and delete, and each entry
          is sealed with a fingerprint of the one before it. Someone with the
          skill to rewrite the database and every seal still cannot change a
          code already printed on paper, which is why a printed log is worth
          keeping in the office file.
        </Alert>
      </div>
    </div>
  );
}
