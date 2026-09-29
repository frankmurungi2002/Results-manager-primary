/**
 * FR-B4 (up to three School Admins) and FR-B6 (one Class Teacher, at most one
 * Assistant, the rest Subject Teachers).
 */

import { useCallback, useEffect, useState } from "react";
import { Camera, Copy, KeyRound, UserPlus, UserX, UsersRound } from "lucide-react";

import { PhotoModal } from "../components/PhotoModal";
import { api } from "../lib/api";
import type {
  AssignmentRow,
  ClassRow,
  ClassSubjectRow,
  StreamRow,
  UserSummary,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  EmptyState,
  Modal,
  SelectInput,
  TableSkeleton,
  TextInput,
  cx,
  formatDate,
} from "../components/ui";

const ROLE_LABELS: Record<AssignmentRow["role"], string> = {
  class_teacher: "Class Teacher",
  assistant_class_teacher: "Assistant Class Teacher",
  subject_teacher: "Subject Teacher",
};

export function StaffScreen() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [staff, setStaff] = useState<UserSummary[]>([]);
  const [classes, setClasses] = useState<ClassRow[]>([]);
  const [assignments, setAssignments] = useState<AssignmentRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<"people" | "assignments">("people");

  const [addOpen, setAddOpen] = useState(false);
  const [assignOpen, setAssignOpen] = useState(false);
  const [photoFor, setPhotoFor] = useState<UserSummary | null>(null);
  const [credentials, setCredentials] = useState<{
    name: string;
    username: string;
    password: string;
  } | null>(null);

  const reload = useCallback(async () => {
    try {
      const [loadedStaff, loadedClasses, loadedAssignments] = await Promise.all([
        api.listStaff(),
        api.listClasses(),
        api.listAssignments(),
      ]);
      setStaff(loadedStaff);
      setClasses(loadedClasses);
      setAssignments(loadedAssignments);
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const admins = staff.filter((person) => person.role === "school_admin");
  const teachers = staff.filter((person) => person.role === "teacher");

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Staff</h1>
            <p className="page-description">
              Accounts are retired, never deleted, so the activity log can still
              name whoever made a change years later.
            </p>
          </div>
          <div className="page-actions">
            <Button
              icon={<UsersRound size={15} />}
              onClick={() => setAssignOpen(true)}
              disabled={teachers.length === 0 || classes.length === 0}
            >
              Assign to a class
            </Button>
            <Button
              variant="primary"
              icon={<UserPlus size={15} />}
              onClick={() => setAddOpen(true)}
            >
              Add staff
            </Button>
          </div>
        </div>

        <div className="tabs">
          <button
            className={cx("tab", tab === "people" && "is-active")}
            onClick={() => setTab("people")}
          >
            People ({staff.length})
          </button>
          <button
            className={cx("tab", tab === "assignments" && "is-active")}
            onClick={() => setTab("assignments")}
          >
            Class assignments ({assignments.length})
          </button>
        </div>

        {admins.length >= 3 && tab === "people" && (
          <Alert tone="info">
            This school has the maximum of three School Admins. Retire one
            before adding another.
          </Alert>
        )}

        {loading ? (
          <Card flush>
            <TableSkeleton rows={6} columns={4} />
          </Card>
        ) : tab === "people" ? (
          <Card flush>
            {staff.length === 0 ? (
              <EmptyState icon={<UsersRound size={18} />} title="No staff yet" />
            ) : (
              <div className="table-wrap">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th style={{ width: 160 }}>Username</th>
                      <th style={{ width: 160 }}>Role</th>
                      <th style={{ width: 160 }}>Last signed in</th>
                      <th style={{ width: 190 }} />
                    </tr>
                  </thead>
                  <tbody>
                    {staff.map((person) => (
                      <tr key={person.id}>
                        <td>
                          <div style={{ fontWeight: 500 }}>{person.fullName}</div>
                          {person.email && (
                            <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                              {person.email}
                            </div>
                          )}
                        </td>
                        <td className="mono muted">{person.username}</td>
                        <td>
                          <Badge
                            tone={person.role === "school_admin" ? "accent" : "neutral"}
                          >
                            {person.role === "school_admin" ? "School Admin" : "Teacher"}
                          </Badge>
                          {person.mustChangePassword && (
                            <div style={{ marginTop: 4 }}>
                              <Badge tone="warning">Password not yet changed</Badge>
                            </div>
                          )}
                        </td>
                        <td className="muted">{formatDate(person.lastLoginAt)}</td>
                        <td>
                          <div className="row" style={{ gap: 2, justifyContent: "flex-end" }}>
                            <Button
                              size="sm"
                              variant="ghost"
                              icon={<Camera size={14} />}
                              title="Photo"
                              onClick={() => setPhotoFor(person)}
                            />
                            <Button
                              size="sm"
                              variant="ghost"
                              icon={<KeyRound size={14} />}
                              title="Reset password"
                              onClick={() => {
                                if (
                                  !window.confirm(
                                    `Reset ${person.fullName}'s password? They will get a new one to sign in with.`,
                                  )
                                )
                                  return;
                                api
                                  .resetUserPassword(person.id)
                                  .then((password) =>
                                    setCredentials({
                                      name: person.fullName,
                                      username: person.username,
                                      password,
                                    }),
                                  )
                                  .catch(reportError);
                              }}
                            />
                            <Button
                              size="sm"
                              variant="ghost"
                              icon={<UserX size={14} />}
                              title="Retire"
                              onClick={() => {
                                if (
                                  !window.confirm(
                                    `Retire ${person.fullName}? They can no longer sign in, but everything they recorded is kept.`,
                                  )
                                )
                                  return;
                                api
                                  .retireStaff(person.id)
                                  .then(() => {
                                    toast("success", `${person.fullName} retired.`);
                                    void reload();
                                  })
                                  .catch(reportError);
                              }}
                            />
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        ) : (
          <Card flush>
            {assignments.length === 0 ? (
              <EmptyState icon={<UsersRound size={18} />} title="Nobody is assigned yet">
                Every class needs one Class Teacher. Subject Teachers are
                assigned per subject.
              </EmptyState>
            ) : (
              <div className="table-wrap">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Teacher</th>
                      <th style={{ width: 160 }}>Class</th>
                      <th style={{ width: 200 }}>Role</th>
                      <th style={{ width: 180 }}>Subject</th>
                      <th style={{ width: 110 }} />
                    </tr>
                  </thead>
                  <tbody>
                    {assignments.map((entry) => (
                      <tr key={entry.id}>
                        <td style={{ fontWeight: 500 }}>{entry.teacherName}</td>
                        <td>
                          {entry.className}
                          {entry.streamName ? ` — ${entry.streamName}` : ""}
                        </td>
                        <td>
                          <Badge
                            tone={entry.role === "class_teacher" ? "accent" : "neutral"}
                          >
                            {ROLE_LABELS[entry.role]}
                          </Badge>
                        </td>
                        <td className="muted">{entry.subjectName ?? "—"}</td>
                        <td>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              api
                                .unassignTeacher(entry.id)
                                .then(() => {
                                  toast("success", "Assignment removed.");
                                  void reload();
                                })
                                .catch(reportError)
                            }
                          >
                            Remove
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        )}
      </div>

      <PhotoModal
        open={photoFor !== null}
        name={photoFor?.fullName ?? ""}
        load={() => (photoFor ? api.getStaffPhoto(photoFor.id) : Promise.resolve(null))}
        save={(png) => (photoFor ? api.setStaffPhoto(photoFor.id, png) : Promise.resolve())}
        onClose={() => setPhotoFor(null)}
      />

      <AddStaffModal
        open={addOpen}
        adminCount={admins.length}
        onClose={() => setAddOpen(false)}
        onCreated={(result, name) => {
          setAddOpen(false);
          setCredentials({ name, username: result.username, password: result.initialPassword });
          void reload();
        }}
      />

      <AssignModal
        open={assignOpen}
        teachers={teachers}
        classes={classes}
        onClose={() => setAssignOpen(false)}
        onAssigned={() => {
          setAssignOpen(false);
          void reload();
        }}
      />

      <Modal
        open={credentials !== null}
        title="Write this down now"
        description="RM shows an initial password once and stores only a hash of it. There is no way to see it again."
        onClose={() => setCredentials(null)}
        footer={
          <Button variant="primary" onClick={() => setCredentials(null)}>
            I have written it down
          </Button>
        }
      >
        {credentials && (
          <div className="stack">
            <Alert tone="warning">
              Hand these to {credentials.name} in person. They will be asked to
              choose their own password the first time they sign in.
            </Alert>
            <CredentialLine label="Username" value={credentials.username} />
            <CredentialLine label="Password" value={credentials.password} />
          </div>
        )}
      </Modal>
    </div>
  );
}

function CredentialLine({ label, value }: { label: string; value: string }) {
  const toast = useStore((state) => state.toast);

  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div className="row">
        <code
          className="selectable grow"
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-md)",
            padding: "var(--space-3)",
            background: "var(--bg-sunken)",
            border: "1px solid var(--border-subtle)",
            borderRadius: "var(--radius-md)",
            letterSpacing: "0.04em",
          }}
        >
          {value}
        </code>
        <Button
          icon={<Copy size={15} />}
          title="Copy"
          onClick={() => {
            void navigator.clipboard
              .writeText(value)
              .then(() => toast("success", `${label} copied.`))
              .catch(() => toast("error", "Could not copy."));
          }}
        />
      </div>
    </div>
  );
}

function AddStaffModal({
  open,
  adminCount,
  onClose,
  onCreated,
}: {
  open: boolean;
  adminCount: number;
  onClose: () => void;
  onCreated: (
    result: { username: string; initialPassword: string },
    name: string,
  ) => void;
}) {
  const reportError = useStore((state) => state.reportError);

  const [fullName, setFullName] = useState("");
  const [username, setUsername] = useState("");
  const [role, setRole] = useState("teacher");
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setFullName("");
      setUsername("");
      setRole("teacher");
      setEmail("");
      setPhone("");
    }
  }, [open]);

  // Suggest a username from the name, so nobody has to invent a convention.
  useEffect(() => {
    const parts = fullName.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (parts.length >= 2) {
      setUsername(`${parts[0]![0]}${parts[parts.length - 1]}`.replace(/[^a-z0-9]/g, ""));
    } else if (parts.length === 1) {
      setUsername(parts[0]!.replace(/[^a-z0-9]/g, ""));
    }
  }, [fullName]);

  return (
    <Modal
      open={open}
      title="Add a staff member"
      description="RM generates a password for them; you hand it over and they change it at first sign-in."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={fullName.trim().length < 2 || username.trim().length < 3}
            onClick={() => {
              setBusy(true);
              api
                .createStaff({
                  fullName,
                  username,
                  role,
                  email: email.trim() || null,
                  phone: phone.trim() || null,
                })
                .then((result) => onCreated(result, fullName.trim()))
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Create account
          </Button>
        </>
      }
    >
      <div className="grid-form">
        <TextInput
          label="Full name"
          className="span-2"
          value={fullName}
          onChange={(event) => setFullName(event.target.value)}
          autoFocus
        />
        <TextInput
          label="Username"
          value={username}
          onChange={(event) => setUsername(event.target.value)}
          spellCheck={false}
        />
        <SelectInput
          label="Role"
          value={role}
          onChange={(event) => setRole(event.target.value)}
          hint={
            adminCount >= 3 ? "The three School Admin places are taken." : undefined
          }
        >
          <option value="teacher">Teacher</option>
          <option value="school_admin" disabled={adminCount >= 3}>
            School Admin
          </option>
        </SelectInput>
        <TextInput
          label="Email (optional)"
          type="email"
          value={email}
          onChange={(event) => setEmail(event.target.value)}
        />
        <TextInput
          label="Phone (optional)"
          value={phone}
          onChange={(event) => setPhone(event.target.value)}
        />
      </div>
    </Modal>
  );
}

function AssignModal({
  open,
  teachers,
  classes,
  onClose,
  onAssigned,
}: {
  open: boolean;
  teachers: UserSummary[];
  classes: ClassRow[];
  onClose: () => void;
  onAssigned: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [userId, setUserId] = useState("");
  const [classId, setClassId] = useState("");
  const [role, setRole] = useState<AssignmentRow["role"]>("subject_teacher");
  const [subjectId, setSubjectId] = useState("");
  const [subjects, setSubjects] = useState<ClassSubjectRow[]>([]);
  const [busy, setBusy] = useState(false);
  const features = useStore((state) => state.features);
  const [streams, setStreams] = useState<StreamRow[]>([]);
  const [streamId, setStreamId] = useState("");

  useEffect(() => {
    if (!open) return;
    setUserId(teachers[0]?.id ?? "");
    setClassId(classes[0]?.id ?? "");
    setRole("subject_teacher");
  }, [open, teachers, classes]);

  // FR-C11: a Class Teacher can be given one stream of the class.
  useEffect(() => {
    setStreamId("");
    if (!classId || !features.streams) {
      setStreams([]);
      return;
    }
    api.listStreams(classId).then(setStreams).catch(() => setStreams([]));
  }, [classId, features.streams]);

  useEffect(() => {
    if (!classId) return;
    api
      .listClassSubjects(classId)
      .then((loaded) => {
        setSubjects(loaded);
        setSubjectId(loaded[0]?.subjectId ?? "");
      })
      .catch(() => undefined);
  }, [classId]);

  return (
    <Modal
      open={open}
      title="Assign a teacher"
      description="A class has exactly one Class Teacher and at most one Assistant. Everyone else teaches a subject."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!userId || !classId || (role === "subject_teacher" && !subjectId)}
            onClick={() => {
              setBusy(true);
              api
                .assignTeacher({
                  userId,
                  classId,
                  streamId: streamId || null,
                  role,
                  subjectId: role === "subject_teacher" ? subjectId : null,
                })
                .then(() => {
                  toast("success", "Teacher assigned.");
                  onAssigned();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Assign
          </Button>
        </>
      }
    >
      <div className="stack">
        <SelectInput
          label="Teacher"
          value={userId}
          onChange={(event) => setUserId(event.target.value)}
        >
          {teachers.map((person) => (
            <option key={person.id} value={person.id}>
              {person.fullName}
            </option>
          ))}
        </SelectInput>

        <SelectInput
          label="Class"
          value={classId}
          onChange={(event) => setClassId(event.target.value)}
        >
          {classes.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </SelectInput>

        <SelectInput
          label="Role"
          value={role}
          onChange={(event) => setRole(event.target.value as AssignmentRow["role"])}
        >
          <option value="class_teacher">Class Teacher</option>
          <option value="assistant_class_teacher">Assistant Class Teacher</option>
          <option value="subject_teacher">Subject Teacher</option>
        </SelectInput>

        {streams.length > 0 && (
          <SelectInput
            label="Stream"
            value={streamId}
            onChange={(event) => setStreamId(event.target.value)}
          >
            <option value="">The whole class</option>
            {streams.map((stream) => (
              <option key={stream.id} value={stream.id}>
                {stream.name}
              </option>
            ))}
          </SelectInput>
        )}

        {role === "subject_teacher" && (
          <SelectInput
            label="Subject"
            value={subjectId}
            onChange={(event) => setSubjectId(event.target.value)}
          >
            {subjects.length === 0 && <option value="">This class has no subjects</option>}
            {subjects.map((subject) => (
              <option key={subject.id} value={subject.subjectId}>
                {subject.displayName}
                {subject.teacherName ? ` — currently ${subject.teacherName}` : ""}
              </option>
            ))}
          </SelectInput>
        )}
      </div>
    </Modal>
  );
}
