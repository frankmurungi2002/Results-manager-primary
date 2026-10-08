/**
 * FR-G23 — bulk SMS. Write one message and send it to the whole school,
 * classes, streams, groups (the football team, the choir) or staff.
 *
 * The backend decides who may text whom and what it costs; this screen only
 * shows it. The preview is live: the count of numbers and SMS on the Send
 * button is the count the backend will charge, and sending repeats it, so a
 * changed audience sends nothing rather than something unexpected.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  MessageSquare,
  Plus,
  RefreshCw,
  Search,
  Send,
  Trash2,
  UserPlus,
  UsersRound,
  X,
} from "lucide-react";

import { ApiError, api } from "../lib/api";
import type {
  AudienceOptions,
  AudiencePart,
  BroadcastPreview,
  BroadcastRow,
  GroupMember,
  GroupRow,
  StudentRow,
  UserSummary,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Loading,
  Modal,
  Progress,
  SelectInput,
  StatTile,
  TableSkeleton,
  TextInput,
  cx,
  formatDateTime,
} from "../components/ui";

type Tab = "compose" | "sent" | "groups";

/** A chosen part of the audience, with the words to show on its chip. */
interface Chosen extends AudiencePart {
  label: string;
}

const key = (part: AudiencePart) => `${part.kind}:${part.id ?? ""}`;

export function MessagesScreen() {
  const session = useStore((state) => state.session);
  const [tab, setTab] = useState<Tab>("compose");

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Messages</h1>
            <p className="page-description">
              Text parents and staff: the whole school, a class, a stream or a group such as
              the football team. A parent with several children here gets one message, and
              you see exactly what it costs before anything is sent.
            </p>
          </div>
        </div>

        <div className="tabs">
          {(
            [
              ["compose", "New message"],
              ["sent", "Sent"],
              ["groups", "Groups"],
            ] as [Tab, string][]
          ).map(([id, label]) => (
            <button key={id} className={cx("tab", tab === id && "is-active")} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </div>

        {tab === "compose" && <Compose onSent={() => setTab("sent")} />}
        {tab === "sent" && <SentList />}
        {tab === "groups" && <Groups isAdmin={Boolean(session?.isAdmin)} />}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// New message
// ---------------------------------------------------------------------------

function Compose({ onSent }: { onSent: () => void }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const navigate = useStore((state) => state.navigate);
  const isAdmin = useStore((state) => Boolean(state.session?.isAdmin));

  const [options, setOptions] = useState<AudienceOptions | null>(null);
  const [chosen, setChosen] = useState<Chosen[]>([]);
  const [message, setMessage] = useState("");
  const [preview, setPreview] = useState<BroadcastPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [sending, setSending] = useState(false);
  const [openClass, setOpenClass] = useState<string | null>(null);
  const [showMissing, setShowMissing] = useState(false);
  const textArea = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    api.listSmsAudiences().then(setOptions).catch(reportError);
  }, [reportError]);

  const audience: AudiencePart[] = useMemo(
    () => chosen.map(({ kind, id }) => ({ kind, id })),
    [chosen],
  );

  // A live preview, a moment after typing stops.
  const request = useRef(0);
  useEffect(() => {
    setPreview(null);
    setPreviewError(null);
    if (audience.length === 0 || message.trim() === "") return;
    const ticket = ++request.current;
    setPreviewing(true);
    const handle = window.setTimeout(() => {
      api
        .previewSmsBroadcast(audience, message)
        .then((result) => {
          if (ticket === request.current) setPreview(result);
        })
        .catch((error: unknown) => {
          if (ticket !== request.current) return;
          setPreviewError(error instanceof ApiError ? error.message : String(error));
        })
        .finally(() => {
          if (ticket === request.current) setPreviewing(false);
        });
    }, 400);
    return () => window.clearTimeout(handle);
  }, [audience, message]);

  const isChosen = (part: AudiencePart) => chosen.some((c) => key(c) === key(part));

  const toggle = (part: Chosen) => {
    setChosen((current) =>
      current.some((c) => key(c) === key(part))
        ? current.filter((c) => key(c) !== key(part))
        : [...current, part],
    );
  };

  const insert = (placeholder: string) => {
    const el = textArea.current;
    if (!el) {
      setMessage((m) => m + placeholder);
      return;
    }
    const start = el.selectionStart ?? message.length;
    const end = el.selectionEnd ?? message.length;
    const next = message.slice(0, start) + placeholder + message.slice(end);
    setMessage(next);
    window.requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(start + placeholder.length, start + placeholder.length);
    });
  };

  const send = () => {
    if (!preview) return;
    setSending(true);
    api
      .sendSmsBroadcast(audience, message, preview.units)
      .then((sent) => {
        toast(
          "success",
          `Sending to ${sent.audienceLabel}: ${sent.recipients} number${sent.recipients === 1 ? "" : "s"}, ${sent.units} SMS.`,
        );
        setConfirming(false);
        setChosen([]);
        setMessage("");
        onSent();
      })
      .catch((error: unknown) => {
        setConfirming(false);
        // The audience changed since the preview: show the new numbers.
        if (error instanceof ApiError && error.code === "conflict") {
          toast("warning", error.message);
          api.previewSmsBroadcast(audience, message).then(setPreview).catch(reportError);
        } else {
          reportError(error);
        }
      })
      .finally(() => setSending(false));
  };

  if (!options) return <Loading label="Loading who you can text" />;

  const nothingToChoose =
    !options.canTextSchool && !options.canTextStaff && options.classes.length === 0 && options.groups.length === 0;

  if (nothingToChoose) {
    return (
      <EmptyState icon={<MessageSquare size={22} />} title="Nobody for you to text yet">
        {options.blockedReason ??
          "You can text the parents of a class you are Class Teacher of, or a group you are in charge of."}
      </EmptyState>
    );
  }

  const usesParentWords = /\{\s*(learner|class)\s*\}/i.test(message);
  const hasStaff = chosen.some((c) => c.kind === "staff");
  const hasParents = chosen.some((c) => c.kind !== "staff");

  return (
    <div className="msg-layout">
      <div className="stack">
        {!options.smsReady && (
          <Alert tone="warning" title="SMS is not set up yet">
            Messages will wait in the outbox and go out as soon as an SMS provider is saved.
            {isAdmin && (
              <>
                {" "}
                <button className="link" onClick={() => navigate("settings")}>
                  Set up SMS in Settings
                </button>
              </>
            )}
          </Alert>
        )}

        <Card title="Who to text" subtitle="Pick one or more. Anyone in two of them is texted once.">
          <div className="stack">
            {(options.canTextSchool || options.canTextStaff) && (
              <div className="msg-options">
                {options.canTextSchool && (
                  <Option
                    label="Whole school"
                    meta="every learner's parent"
                    on={isChosen({ kind: "school", id: null })}
                    onClick={() => toggle({ kind: "school", id: null, label: "Whole school" })}
                  />
                )}
                {options.canTextStaff && (
                  <Option
                    label="All staff"
                    meta="every active staff member"
                    on={isChosen({ kind: "staff", id: null })}
                    onClick={() => toggle({ kind: "staff", id: null, label: "All staff" })}
                  />
                )}
              </div>
            )}

            {options.classes.length > 0 && (
              <div>
                <div className="field-label">Classes</div>
                <div className="msg-classes">
                  {options.classes.map((klass) => (
                    <div key={klass.id} className="msg-class">
                      <div className="msg-class-row">
                        <Option
                          label={klass.name}
                          meta={`${klass.learners} learner${klass.learners === 1 ? "" : "s"}`}
                          on={isChosen({ kind: "class", id: klass.id })}
                          onClick={() => toggle({ kind: "class", id: klass.id, label: klass.name })}
                        />
                        {klass.streams.length > 0 && (
                          <Button
                            size="sm"
                            variant="ghost"
                            icon={openClass === klass.id ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                            title={`${klass.streams.length} streams`}
                            aria-label={`${klass.name} streams`}
                            onClick={() => setOpenClass(openClass === klass.id ? null : klass.id)}
                          >
                            {klass.streams.length}
                          </Button>
                        )}
                      </div>
                      {openClass === klass.id && (
                        <div className="msg-options msg-streams">
                          {klass.streams.map((stream) => (
                            <Option
                              key={stream.id}
                              label={stream.name}
                              on={isChosen({ kind: "stream", id: stream.id })}
                              onClick={() =>
                                toggle({ kind: "stream", id: stream.id, label: `${klass.name} ${stream.name}` })
                              }
                            />
                          ))}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {options.groups.length > 0 && (
              <div>
                <div className="field-label">Groups</div>
                <div className="msg-options">
                  {options.groups.map((group) => (
                    <Option
                      key={group.id}
                      label={group.name}
                      meta={`${group.members} member${group.members === 1 ? "" : "s"}`}
                      on={isChosen({ kind: "group", id: group.id })}
                      onClick={() => toggle({ kind: "group", id: group.id, label: group.name })}
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        </Card>

        <Card title="Message">
          <div className="stack">
            <textarea
              ref={textArea}
              className="textarea"
              aria-label="Message"
              rows={5}
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              placeholder={
                hasStaff && !hasParents
                  ? "Dear {name}, there is a staff meeting at 4pm in the library."
                  : "Dear parent of {learner}, school closes for the term on Friday at 12 noon."
              }
            />
            <div className="row msg-placeholders">
              <span className="subtle">Insert:</span>
              {hasParents && (
                <>
                  <Button size="sm" variant="ghost" onClick={() => insert("{learner}")}>
                    {"{learner}"}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => insert("{class}")}>
                    {"{class}"}
                  </Button>
                </>
              )}
              {hasStaff && !usesParentWords && (
                <Button size="sm" variant="ghost" onClick={() => insert("{name}")}>
                  {"{name}"}
                </Button>
              )}
              <span className="grow" />
              <span className="subtle mono">{message.length} characters</span>
            </div>
            <p className="field-hint">
              {"{learner}"} becomes the child's name — or "Amina and Musa" for a parent of two.
              Every message starts with your school's SMS name.
            </p>
          </div>
        </Card>
      </div>

      <div className="stack msg-side">
        <Card title="Before you send">
          {chosen.length === 0 || message.trim() === "" ? (
            <p className="muted">Choose who to text and write the message to see who it reaches and what it costs.</p>
          ) : previewError ? (
            <Alert tone="danger">{previewError}</Alert>
          ) : !preview || previewing ? (
            <Loading label="Counting" />
          ) : (
            <div className="stack">
              <div className="msg-chips">
                {chosen.map((c) => (
                  <span key={key(c)} className="msg-chip">
                    {c.label}
                    <button aria-label={`Remove ${c.label}`} onClick={() => toggle(c)}>
                      <X size={12} />
                    </button>
                  </span>
                ))}
              </div>
              <div className="grid grid-stats msg-stats">
                <StatTile label="Phone numbers" value={preview.recipients} meta={`${preview.learners} learners`} />
                <StatTile
                  label="SMS charged"
                  value={preview.units}
                  meta={`${preview.partsPerMessage} per message${preview.encoding === "unicode" ? " · Unicode" : ""}`}
                />
              </div>
              {preview.sample && (
                <div>
                  <div className="field-label">What a parent will read</div>
                  <div className="msg-bubble">{preview.sample}</div>
                </div>
              )}
              {preview.notes.map((note) => (
                <Alert key={note} tone={note.includes("Unicode") || note.includes("70 characters") ? "warning" : "info"}>
                  {note}
                </Alert>
              ))}
              {preview.missingCount > 0 && (
                <Alert tone="warning" title={`${preview.missingCount} cannot be texted`}>
                  No phone number that can receive an SMS.{" "}
                  <button className="link" onClick={() => setShowMissing((v) => !v)}>
                    {showMissing ? "Hide" : "Show who"}
                  </button>
                  {showMissing && (
                    <ul className="msg-missing">
                      {preview.missingNames.map((name) => (
                        <li key={name}>{name}</li>
                      ))}
                      {preview.missingCount > preview.missingNames.length && (
                        <li className="subtle">and {preview.missingCount - preview.missingNames.length} more</li>
                      )}
                    </ul>
                  )}
                </Alert>
              )}
              <Button
                variant="primary"
                block
                icon={<Send size={15} />}
                disabled={preview.recipients === 0}
                onClick={() => setConfirming(true)}
              >
                Send to {preview.recipients} number{preview.recipients === 1 ? "" : "s"}
              </Button>
            </div>
          )}
        </Card>
      </div>

      <Modal
        open={confirming && preview !== null}
        title="Send this message?"
        description={preview ? `To ${preview.audienceLabel}` : undefined}
        onClose={() => setConfirming(false)}
        footer={
          <>
            <Button onClick={() => setConfirming(false)}>Not yet</Button>
            <Button variant="primary" loading={sending} icon={<Send size={15} />} onClick={send}>
              Send {preview?.units} SMS
            </Button>
          </>
        }
      >
        {preview && (
          <div className="stack">
            <p>
              <strong>{preview.recipients}</strong> phone number{preview.recipients === 1 ? "" : "s"} will
              be texted, costing <strong>{preview.units} SMS</strong> from the school's account. A text
              cannot be taken back once it is sent.
            </p>
            {preview.sample && <div className="msg-bubble">{preview.sample}</div>}
          </div>
        )}
      </Modal>
    </div>
  );
}

function Option({
  label,
  meta,
  on,
  onClick,
}: {
  label: string;
  meta?: string;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button className={cx("msg-option", on && "is-on")} aria-pressed={on} onClick={onClick}>
      <span className="msg-option-label">{label}</span>
      {meta && <span className="msg-option-meta">{meta}</span>}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Sent
// ---------------------------------------------------------------------------

function SentList() {
  const reportError = useStore((state) => state.reportError);
  const [rows, setRows] = useState<BroadcastRow[] | null>(null);

  const load = useCallback(() => {
    api.listSmsBroadcasts().then(setRows).catch(reportError);
  }, [reportError]);

  useEffect(() => {
    load();
    // Copies leave the outbox in the background; keep the counts moving.
    const handle = window.setInterval(load, 15_000);
    return () => window.clearInterval(handle);
  }, [load]);

  return (
    <Card
      title="Sent messages"
      subtitle="Each message and how far it has got"
      actions={
        <Button size="sm" variant="ghost" icon={<RefreshCw size={14} />} onClick={load}>
          Refresh
        </Button>
      }
      flush
    >
      {rows === null ? (
        <TableSkeleton rows={5} columns={4} />
      ) : rows.length === 0 ? (
        <EmptyState icon={<MessageSquare size={22} />} title="Nothing sent yet" />
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>To</th>
                <th>Message</th>
                <th style={{ width: 220 }}>Delivered to the gateway</th>
                <th style={{ width: 160 }}>Sent by</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.id}>
                  <td>
                    <div>{row.audienceLabel.charAt(0).toUpperCase() + row.audienceLabel.slice(1)}</div>
                    <div className="subtle">
                      {row.recipients} numbers · {row.units} SMS
                    </div>
                  </td>
                  <td className="msg-template">{row.template}</td>
                  <td>
                    <Progress value={row.sent} max={row.recipients} />
                    <div className="row" style={{ marginTop: 6, gap: 6 }}>
                      <Badge tone="success">{row.sent} sent</Badge>
                      {row.waiting > 0 && <Badge tone="info">{row.waiting} waiting</Badge>}
                      {row.failed > 0 && <Badge tone="danger">{row.failed} failed</Badge>}
                    </div>
                  </td>
                  <td>
                    <div>{row.sentBy}</div>
                    <div className="subtle">{formatDateTime(row.createdAt)}</div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

function Groups({ isAdmin }: { isAdmin: boolean }) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [groups, setGroups] = useState<GroupRow[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<GroupRow | "new" | null>(null);

  const load = useCallback(() => {
    api
      .listLearnerGroups()
      .then((rows) => {
        setGroups(rows);
        setSelected((current) => current ?? rows[0]?.id ?? null);
      })
      .catch(reportError);
  }, [reportError]);

  useEffect(load, [load]);

  const current = groups?.find((g) => g.id === selected) ?? null;

  return (
    <div className="msg-layout">
      <Card
        title="Groups"
        subtitle="Learners who are not a class: teams, clubs, choirs"
        actions={
          isAdmin && (
            <Button size="sm" variant="primary" icon={<Plus size={14} />} onClick={() => setEditing("new")}>
              New group
            </Button>
          )
        }
        flush
      >
        {groups === null ? (
          <TableSkeleton rows={4} columns={2} />
        ) : groups.length === 0 ? (
          <EmptyState icon={<UsersRound size={22} />} title="No groups yet">
            {isAdmin
              ? "Make one for the football team, the choir or the debate club, then text it from New message."
              : "Groups you are put in charge of appear here."}
          </EmptyState>
        ) : (
          <div className="msg-group-list">
            {groups.map((group) => (
              <button
                key={group.id}
                className={cx("msg-group", group.id === selected && "is-on")}
                onClick={() => setSelected(group.id)}
              >
                <span className="msg-option-label">{group.name}</span>
                <span className="msg-option-meta">
                  {group.members} member{group.members === 1 ? "" : "s"}
                  {group.leaderName ? ` · ${group.leaderName}` : ""}
                </span>
              </button>
            ))}
          </div>
        )}
      </Card>

      {current && (
        <Members
          key={current.id}
          group={current}
          isAdmin={isAdmin}
          onEdit={() => setEditing(current)}
          onChanged={load}
          onRetired={() => {
            toast("success", `${current.name} retired.`);
            setSelected(null);
            load();
          }}
        />
      )}

      {editing && (
        <GroupEditor
          group={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={(id) => {
            setEditing(null);
            setSelected(id);
            load();
          }}
        />
      )}
    </div>
  );
}

function Members({
  group,
  isAdmin,
  onEdit,
  onChanged,
  onRetired,
}: {
  group: GroupRow;
  isAdmin: boolean;
  onEdit: () => void;
  onChanged: () => void;
  onRetired: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [members, setMembers] = useState<GroupMember[] | null>(null);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<StudentRow[]>([]);
  const [retiring, setRetiring] = useState(false);

  const load = useCallback(() => {
    api.listGroupMembers(group.id).then(setMembers).catch(reportError);
  }, [group.id, reportError]);

  useEffect(load, [load]);

  useEffect(() => {
    if (query.trim().length < 2) {
      setMatches([]);
      return;
    }
    const handle = window.setTimeout(() => {
      api.searchStudents(query).then(setMatches).catch(reportError);
    }, 250);
    return () => window.clearTimeout(handle);
  }, [query, reportError]);

  const memberIds = new Set(members?.map((m) => m.studentId));

  const change = (add: string[], remove: string[]) => {
    api
      .changeGroupMembers(group.id, add, remove)
      .then(() => {
        load();
        onChanged();
      })
      .catch(reportError);
  };

  const withoutPhone = members?.filter((m) => !m.hasPhone).length ?? 0;

  return (
    <Card
      title={group.name}
      subtitle={group.note ?? (group.leaderName ? `In charge: ${group.leaderName}` : "No teacher in charge")}
      actions={
        isAdmin && (
          <>
            <Button size="sm" variant="ghost" onClick={onEdit}>
              Edit
            </Button>
            <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => setRetiring(true)}>
              Retire
            </Button>
          </>
        )
      }
    >
      <div className="stack">
        <div>
          <TextInput
            label="Add learners"
            placeholder="Search by name or registration number"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          {matches.length > 0 && (
            <div className="po-matches">
              {matches
                .filter((s) => s.status === "active")
                .slice(0, 8)
                .map((student) => (
                  <button
                    key={student.id}
                    className="po-match"
                    disabled={memberIds.has(student.id)}
                    onClick={() => {
                      change([student.id], []);
                      setQuery("");
                    }}
                  >
                    <span>
                      <UserPlus size={14} /> {student.fullName}
                    </span>
                    <span className="subtle">
                      {memberIds.has(student.id) ? "already in" : (student.className ?? "no class")}
                    </span>
                  </button>
                ))}
            </div>
          )}
        </div>

        {withoutPhone > 0 && (
          <Alert tone="warning">
            {withoutPhone} member{withoutPhone === 1 ? " has" : "s have"} no phone number that can be texted.
          </Alert>
        )}

        {members === null ? (
          <TableSkeleton rows={4} columns={3} />
        ) : members.length === 0 ? (
          <EmptyState icon={<Search size={20} />} title="No members yet">
            Search above to add learners.
          </EmptyState>
        ) : (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Learner</th>
                  <th style={{ width: 90 }}>Class</th>
                  <th style={{ width: 110 }}>Phone</th>
                  <th style={{ width: 60 }} />
                </tr>
              </thead>
              <tbody>
                {members.map((m) => (
                  <tr key={m.studentId}>
                    <td>
                      <div>{m.fullName}</div>
                      <div className="subtle mono">{m.regNumber}</div>
                    </td>
                    <td>{m.className ?? "—"}</td>
                    <td>{m.hasPhone ? <Badge tone="success">Yes</Badge> : <Badge tone="warning">None</Badge>}</td>
                    <td>
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={<X size={14} />}
                        aria-label={`Remove ${m.fullName}`}
                        onClick={() => change([], [m.studentId])}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <Modal
        open={retiring}
        title={`Retire ${group.name}?`}
        description="It disappears from the list and can no longer be texted. Messages already sent keep their record."
        onClose={() => setRetiring(false)}
        footer={
          <>
            <Button onClick={() => setRetiring(false)}>Keep it</Button>
            <Button
              variant="danger"
              onClick={() => {
                api
                  .retireLearnerGroup(group.id)
                  .then(onRetired)
                  .catch(reportError)
                  .finally(() => setRetiring(false));
              }}
            >
              Retire group
            </Button>
          </>
        }
      >
        <p className="muted">Nothing is deleted.</p>
      </Modal>
    </Card>
  );
}

function GroupEditor({
  group,
  onClose,
  onSaved,
}: {
  group: GroupRow | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const [name, setName] = useState(group?.name ?? "");
  const [note, setNote] = useState(group?.note ?? "");
  const [leaderId, setLeaderId] = useState(group?.leaderId ?? "");
  const [staff, setStaff] = useState<UserSummary[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api
      .listStaff()
      .then((rows) => setStaff(rows.filter((r) => r.status === "active")))
      .catch(reportError);
  }, [reportError]);

  const save = () => {
    setSaving(true);
    const input = { name, note: note.trim() || null, leaderId: leaderId || null };
    const done = group
      ? api.updateLearnerGroup(group.id, input).then(() => group.id)
      : api.createLearnerGroup(input);
    done
      .then(onSaved)
      .catch(reportError)
      .finally(() => setSaving(false));
  };

  return (
    <Modal
      open
      title={group ? `Edit ${group.name}` : "New group"}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={saving} disabled={name.trim() === ""} onClick={save}>
            {group ? "Save" : "Create group"}
          </Button>
        </>
      }
    >
      <div className="stack">
        <TextInput label="Name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Football team" />
        <TextInput
          label="Note"
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder="Under-12 boys, trains Tuesday and Thursday"
        />
        <SelectInput
          label="Teacher in charge"
          value={leaderId}
          onChange={(e) => setLeaderId(e.target.value)}
          hint="They can keep the member list and text the group's parents."
        >
          <option value="">No one</option>
          {staff.map((s) => (
            <option key={s.id} value={s.id}>
              {s.fullName}
            </option>
          ))}
        </SelectInput>
      </div>
    </Modal>
  );
}
