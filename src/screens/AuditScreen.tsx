/**
 * FR-E2 — the activity log.
 *
 * Append-only in the database, searchable here. It is precise enough to tell a
 * Class Teacher's change from their Assistant's, which is the point of it.
 */

import { useCallback, useEffect, useState } from "react";
import { ScrollText, Search } from "lucide-react";

import { api } from "../lib/api";
import type { AuditEntry } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  TableSkeleton,
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

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Activity log</h1>
            <p className="page-description">
              Every action by every person, with a timestamp. Entries cannot be
              edited or removed — the database refuses it.
            </p>
          </div>
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
                    <tr key={entry.id}>
                      <td className="muted">{formatDateTime(entry.at)}</td>
                      <td>
                        {entry.actor_name ?? <span className="subtle">System</span>}
                        {entry.actor_role && (
                          <div style={{ marginTop: 2 }}>
                            <Badge
                              tone={
                                entry.actor_role === "school_admin" ? "accent" : "neutral"
                              }
                            >
                              {entry.actor_role === "school_admin"
                                ? "School Admin"
                                : "Teacher"}
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
          keeping. The audit table rejects every update and delete at the
          database level, not just in the application.
        </Alert>
      </div>
    </div>
  );
}
