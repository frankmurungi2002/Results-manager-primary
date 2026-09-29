/**
 * FR-C11 — a class's streams, shown on the Classes screen while the Streams
 * toggle is on. Up to 20 per class; each gets its own roster (Learners) and
 * its own Class Teacher (Staff).
 */

import { useCallback, useEffect, useState } from "react";
import { Pencil, Plus, Split, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { StreamRow } from "../lib/types";
import { useStore } from "../state/store";
import { Button, Card, EmptyState, TextInput } from "./ui";

export function StreamsCard({ classId, className }: { classId: string; className: string }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const isAdmin = useStore((state) => state.session?.isAdmin ?? false);

  const [streams, setStreams] = useState<StreamRow[]>([]);
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.listStreams(classId).then(setStreams).catch(reportError);
  }, [classId, reportError]);

  useEffect(() => {
    setName("");
    setEditing(null);
    load();
  }, [load]);

  function submit() {
    if (!name.trim()) return;
    setBusy(true);
    api
      .saveStream(classId, name.trim(), editing)
      .then(() => {
        toast("success", editing ? "Stream renamed." : `Stream ${name.trim()} added to ${className}.`);
        setName("");
        setEditing(null);
        load();
      })
      .catch(reportError)
      .finally(() => setBusy(false));
  }

  return (
    <Card
      title="Streams"
      subtitle={`${streams.length} of 20 • each with its own roster and Class Teacher`}
      flush
      footer={
        isAdmin ? (
          <div className="row" style={{ alignItems: "flex-end" }}>
            <div className="grow">
              <TextInput
                label={editing ? "New name" : "Add a stream"}
                value={name}
                placeholder="e.g. Blue, East, A"
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") submit();
                }}
              />
            </div>
            {editing && (
              <Button
                variant="ghost"
                onClick={() => {
                  setEditing(null);
                  setName("");
                }}
              >
                Cancel
              </Button>
            )}
            <Button
              variant="primary"
              icon={editing ? <Pencil size={15} /> : <Plus size={15} />}
              loading={busy}
              disabled={!name.trim() || (!editing && streams.length >= 20)}
              onClick={submit}
            >
              {editing ? "Rename" : "Add stream"}
            </Button>
          </div>
        ) : undefined
      }
    >
      {streams.length === 0 ? (
        <EmptyState icon={<Split size={18} />} title="No streams yet">
          The whole class shares one roster until you add a stream.
        </EmptyState>
      ) : (
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th>Stream</th>
                <th style={{ width: 110 }}>Learners</th>
                <th>Class Teacher</th>
                {isAdmin && <th style={{ width: 90 }} />}
              </tr>
            </thead>
            <tbody>
              {streams.map((stream) => (
                <tr key={stream.id}>
                  <td style={{ fontWeight: 600 }}>{stream.name}</td>
                  <td>{stream.learnerCount}</td>
                  <td className={stream.classTeacherName ? "" : "subtle"}>
                    {stream.classTeacherName ?? "Not assigned (Staff screen)"}
                  </td>
                  {isAdmin && (
                    <td>
                      <div className="row" style={{ gap: 2, justifyContent: "flex-end" }}>
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<Pencil size={13} />}
                          title="Rename"
                          onClick={() => {
                            setEditing(stream.id);
                            setName(stream.name);
                          }}
                        />
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<Trash2 size={13} />}
                          title="Remove"
                          onClick={() => {
                            if (!window.confirm(`Remove stream ${stream.name}?`)) return;
                            api
                              .retireStream(stream.id)
                              .then(() => {
                                toast("success", `Stream ${stream.name} removed.`);
                                load();
                              })
                              .catch(reportError);
                          }}
                        />
                      </div>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
