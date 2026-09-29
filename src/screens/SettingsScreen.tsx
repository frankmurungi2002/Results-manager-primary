/**
 * FR-B2 (institution identity and logo), FR-B7 (the optional features panel),
 * FR-B9 (theme), FR-B10 (backup status) and the account's own password.
 */

import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  DatabaseBackup,
  FolderOpen,
  HardDrive,
  History,
  Image as ImageIcon,
  Palette,
  ShieldCheck,
  ToggleRight,
  Trash2,
} from "lucide-react";

import { ApiError, api } from "../lib/api";
import type {
  BackupFile,
  BackupStatus,
  BackupSummary,
  FeatureFlags,
  Institution,
  SmsOutboxRow,
  SmsSettings,
} from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  Loading,
  Modal,
  Segmented,
  SelectInput,
  Switch,
  TextArea,
  TextInput,
  cx,
  formatDateTime,
  relativeTime,
} from "../components/ui";

type Tab = "school" | "features" | "sms" | "backup" | "account";

const ACCENTS = [
  "#4F63D2",
  "#0E8577",
  "#9B2C3A",
  "#2A6B3F",
  "#1F3A70",
  "#6B3A82",
  "#B45309",
  "#0F766E",
];

export function SettingsScreen() {
  const session = useStore((state) => state.session);
  const params = useStore((state) => state.screenParams);
  const [tab, setTab] = useState<Tab>((params.tab as Tab) ?? "school");

  const isAdmin = session?.isAdmin ?? false;

  const tabs: { id: Tab; label: string; adminOnly: boolean }[] = [
    { id: "school", label: "School", adminOnly: true },
    { id: "features", label: "Optional features", adminOnly: true },
    { id: "sms", label: "SMS", adminOnly: true },
    { id: "backup", label: "Backup", adminOnly: true },
    { id: "account", label: "My account", adminOnly: false },
  ];

  const visible = tabs.filter((entry) => !entry.adminOnly || isAdmin);

  useEffect(() => {
    if (!visible.some((entry) => entry.id === tab)) setTab("account");
  }, [visible, tab]);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Settings</h1>
            <p className="page-description">
              Anything here can be changed at any time without affecting records
              already saved.
            </p>
          </div>
        </div>

        <div className="tabs">
          {visible.map((entry) => (
            <button
              key={entry.id}
              className={cx("tab", tab === entry.id && "is-active")}
              onClick={() => setTab(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>

        {tab === "school" && isAdmin && <SchoolTab />}
        {tab === "features" && isAdmin && <FeaturesTab />}
        {tab === "sms" && isAdmin && <SmsTab />}
        {tab === "backup" && isAdmin && <BackupTab />}
        {tab === "account" && <AccountTab />}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// FR-B2 — the school's identity
// ---------------------------------------------------------------------------

function SchoolTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const refreshInstitution = useStore((state) => state.refreshInstitution);
  const theme = useStore((state) => state.theme);
  const setTheme = useStore((state) => state.setTheme);

  const [institution, setInstitution] = useState<Institution | null>(null);
  const [name, setName] = useState("");
  const [motto, setMotto] = useState("");
  const [address, setAddress] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");
  const [accentColor, setAccentColor] = useState("#4F63D2");
  const [logo, setLogo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getInstitution()
      .then((loaded) => {
        setInstitution(loaded);
        setName(loaded.name);
        setMotto(loaded.motto ?? "");
        setAddress(loaded.address ?? "");
        setPhone(loaded.phone ?? "");
        setEmail(loaded.email ?? "");
        setAccentColor(loaded.accentColor);
      })
      .catch(reportError);
  }, [reportError]);

  useEffect(() => {
    if (!institution?.hasLogo) return;
    api
      .getInstitutionLogo()
      .then((bytes) => {
        if (!bytes) return;
        setLogo(
          URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" })),
        );
      })
      .catch(() => undefined);
  }, [institution?.hasLogo]);

  async function chooseLogo() {
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "PNG image", extensions: ["png"] }],
      });
      if (typeof path !== "string") return;

      // The backend reads the file by path and checks it is a PNG under 2 MB.
      const bytes = await api.readImageFile(path);
      await api.setInstitutionLogo(bytes);
      setLogo(URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" })));
      setInstitution((current) => (current ? { ...current, hasLogo: true } : current));
      toast("success", "Logo saved. It now prints on every document.");
      void refreshInstitution();
    } catch (error) {
      reportError(error);
    }
  }

  if (!institution) return <Loading />;

  return (
    <div className="stack">
      <Card
        title="School identity"
        subtitle="This appears at the top of every document RM prints"
        footer={
          <div className="row-between">
            <span className="field-hint">The name is required; a logo never is.</span>
            <Button
              variant="primary"
              loading={busy}
              disabled={name.trim().length < 2}
              onClick={() => {
                setBusy(true);
                api
                  .updateInstitution({
                    name,
                    motto: motto.trim() || null,
                    address: address.trim() || null,
                    phone: phone.trim() || null,
                    email: email.trim() || null,
                    accentColor,
                  })
                  .then(() => {
                    toast("success", "School details saved.");
                    void refreshInstitution();
                  })
                  .catch(reportError)
                  .finally(() => setBusy(false));
              }}
            >
              Save changes
            </Button>
          </div>
        }
      >
        <div className="grid-form">
          <TextInput
            label="School name"
            className="span-2"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <TextInput
            label="Motto"
            value={motto}
            onChange={(event) => setMotto(event.target.value)}
          />
          <TextInput
            label="Telephone"
            value={phone}
            onChange={(event) => setPhone(event.target.value)}
          />
          <TextInput
            label="Address"
            value={address}
            onChange={(event) => setAddress(event.target.value)}
          />
          <TextInput
            label="Email"
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </div>

        <div className="field" style={{ marginTop: "var(--space-5)" }}>
          <span className="field-label">
            <Palette size={12} style={{ display: "inline", verticalAlign: -2 }} /> Accent
            colour
          </span>
          <div className="row" style={{ flexWrap: "wrap", gap: "var(--space-2)" }}>
            {ACCENTS.map((colour) => (
              <button
                key={colour}
                onClick={() => setAccentColor(colour)}
                title={colour}
                style={{
                  width: 30,
                  height: 30,
                  borderRadius: "var(--radius-md)",
                  background: colour,
                  border:
                    accentColor === colour
                      ? "2px solid var(--text-primary)"
                      : "1px solid var(--border-default)",
                  outline:
                    accentColor === colour ? "2px solid var(--bg-surface)" : "none",
                  outlineOffset: -4,
                }}
              />
            ))}
          </div>
        </div>
      </Card>

      <Card title="Logo" subtitle="A PNG up to 2 MB, printed at the top-left of every page">
        <div className="row" style={{ gap: "var(--space-5)" }}>
          <div
            style={{
              width: 88,
              height: 88,
              display: "grid",
              placeItems: "center",
              border: "1px dashed var(--border-default)",
              borderRadius: "var(--radius-md)",
              background: "var(--bg-inset)",
              flex: "none",
              overflow: "hidden",
            }}
          >
            {logo ? (
              <img src={logo} alt="School logo" style={{ maxWidth: "100%", maxHeight: "100%" }} />
            ) : (
              <ImageIcon size={22} className="subtle" />
            )}
          </div>
          <div className="grow">
            <div className="row">
              <Button icon={<ImageIcon size={15} />} onClick={() => void chooseLogo()}>
                Choose a PNG
              </Button>
              {institution.hasLogo && (
                <Button
                  variant="ghost"
                  icon={<Trash2 size={15} />}
                  onClick={() =>
                    api
                      .setInstitutionLogo(null)
                      .then(() => {
                        setLogo(null);
                        setInstitution((current) =>
                          current ? { ...current, hasLogo: false } : current,
                        );
                        toast("success", "Logo removed.");
                        void refreshInstitution();
                      })
                      .catch(reportError)
                  }
                >
                  Remove
                </Button>
              )}
            </div>
            <p className="field-hint" style={{ marginTop: "var(--space-2)" }}>
              A square logo prints best. Documents lay out correctly with or
              without one.
            </p>
          </div>
        </div>
      </Card>

      <LoginPicturesCard />

      <ReportSettingsCard />

      <Card title="Appearance" subtitle="Applies to this computer only">
        <div className="row-between">
          <span>Theme</span>
          <Segmented
            value={theme}
            onChange={(next) => void setTheme(next)}
            options={[
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
              { value: "system", label: "Match Windows" },
            ]}
          />
        </div>
      </Card>
    </div>
  );
}

/** The four pictures that rotate on the sign-in screen. */
function LoginPicturesCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [photos, setPhotos] = useState<(string | null)[] | null>(null);
  const [busy, setBusy] = useState<number | null>(null);

  const load = () => api.getLoginImages().then(setPhotos).catch(reportError);
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function choose(slot: number) {
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Picture", extensions: ["png", "jpg", "jpeg"] }],
      });
      if (typeof path !== "string") return;
      setBusy(slot);
      await api.setLoginImage(slot, path);
      toast("success", "Sign-in picture saved.");
      await load();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(null);
    }
  }

  if (!photos) return null;
  const captions = [
    "Every learner, every mark",
    "Registers and timetables",
    "From Baby Class to PLE",
    "Report cards in minutes",
  ];

  return (
    <Card
      title="Sign-in pictures"
      subtitle="Four pictures rotate on the sign-in screen. Use your own photos, or keep the built-in pictures."
    >
      <div className="login-pics">
        {photos.map((photo, index) => (
          <div key={index} className="login-pic">
            <div className="login-pic-frame">
              {photo ? <img src={photo} alt="" /> : <span>Built-in picture</span>}
            </div>
            <div className="login-pic-caption">{captions[index]}</div>
            <div className="row" style={{ gap: 4 }}>
              <Button size="sm" loading={busy === index + 1} onClick={() => void choose(index + 1)}>
                {photo ? "Change" : "Choose photo"}
              </Button>
              {photo && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    api
                      .setLoginImage(index + 1, null)
                      .then(load)
                      .catch(reportError)
                  }
                >
                  Remove
                </Button>
              )}
            </div>
          </div>
        ))}
      </div>
      <p className="field-hint" style={{ marginTop: "var(--space-3)" }}>
        PNG or JPEG up to 4 MB. A tall photo (portrait) fills the panel best.
      </p>
    </Card>
  );
}

/** What every report card prints beyond marks: signatures and next-term notes. */
function ReportSettingsCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [loaded, setLoaded] = useState(false);
  const [headTeacherName, setHeadTeacherName] = useState("");
  const [requirements, setRequirements] = useState("");
  const [activities, setActivities] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getReportSettings()
      .then((settings) => {
        setHeadTeacherName(settings.headTeacherName);
        setRequirements(settings.requirements);
        setActivities(settings.nurseryActivities.join("\n"));
        setLoaded(true);
      })
      .catch(reportError);
  }, [reportError]);

  if (!loaded) return null;

  return (
    <Card
      title="Report cards"
      subtitle="Printed at the foot of every learner's report"
      footer={
        <div className="row-between">
          <span className="field-hint">
            The next term's start date comes from the term calendar.
          </span>
          <Button
            variant="primary"
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .saveReportSettings({
                  headTeacherName,
                  requirements,
                  nurseryActivities: activities.split("\n"),
                })
                .then(() => toast("success", "Report card settings saved."))
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save
          </Button>
        </div>
      }
    >
      <div className="grid-form">
        <TextInput
          label="Headteacher's name"
          value={headTeacherName}
          onChange={(event) => setHeadTeacherName(event.target.value)}
          placeholder="Printed beside the Headteacher's comment"
        />
        <TextArea
          label="School requirements for next term"
          value={requirements}
          onChange={(event) => setRequirements(event.target.value)}
          placeholder="Broom, ream of paper, box file, 12 books ..."
        />
        <TextArea
          label="Nursery learning activities, one per line"
          value={activities}
          onChange={(event) => setActivities(event.target.value)}
          rows={6}
        />
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// FR-G8 — SMS to guardians
// ---------------------------------------------------------------------------

function SmsTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [settings, setSettings] = useState<SmsSettings | null>(null);
  const [outbox, setOutbox] = useState<SmsOutboxRow[]>([]);
  const [testPhone, setTestPhone] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | "send" | null>(null);

  const loadOutbox = () => api.listSmsOutbox().then(setOutbox).catch(reportError);

  useEffect(() => {
    api.getSmsSettings().then(setSettings).catch(reportError);
    void loadOutbox();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportError]);

  if (!settings) return <Loading />;

  const set = (patch: Partial<SmsSettings>) => setSettings({ ...settings, ...patch });
  const isAt = settings.provider === "africastalking";
  const waiting = outbox.filter((row) => row.status === "queued").length;
  const failed = outbox.filter((row) => row.status === "failed").length;

  return (
    <div className="stack">
      <Card
        title="SMS provider"
        subtitle="Texts to guardians go through a Ugandan SMS gateway. The school pays the gateway directly."
        footer={
          <div className="row-between">
            <span className="field-hint">
              Messages wait in the outbox when there is no internet and send
              themselves when it comes back.
            </span>
            <Button
              variant="primary"
              loading={busy === "save"}
              onClick={() => {
                setBusy("save");
                api
                  .saveSmsSettings(settings)
                  .then(() => {
                    toast("success", "SMS settings saved.");
                    window.setTimeout(() => void loadOutbox(), 4000);
                  })
                  .catch(reportError)
                  .finally(() => setBusy(null));
              }}
            >
              Save
            </Button>
          </div>
        }
      >
        <div className="grid-form">
          <SelectInput
            label="Provider"
            value={settings.provider}
            onChange={(event) =>
              set({ provider: event.target.value as SmsSettings["provider"] })
            }
          >
            <option value="off">Off — don't send texts</option>
            <option value="africastalking">Africa's Talking</option>
            <option value="egosms">EgoSMS</option>
          </SelectInput>
          {settings.provider !== "off" && (
            <>
              <TextInput
                label="Username"
                value={settings.username}
                onChange={(event) => set({ username: event.target.value })}
                hint={isAt ? "Use sandbox to try it out without sending real texts." : undefined}
              />
              <TextInput
                label={isAt ? "API key" : "Password"}
                type="password"
                value={settings.apiKey}
                onChange={(event) => set({ apiKey: event.target.value })}
                onFocus={(event) => event.target.select()}
              />
              <TextInput
                label="Sender ID"
                value={settings.senderId}
                onChange={(event) => set({ senderId: event.target.value })}
                hint="The name texts come from, if the gateway has registered one for you."
              />
              <TextInput
                label="Messages start with"
                value={settings.signature}
                onChange={(event) => set({ signature: event.target.value })}
                placeholder="Your school name"
                hint="A short name keeps each text to one SMS, e.g. RAINBOW N&P."
              />
            </>
          )}
        </div>
      </Card>

      {settings.provider !== "off" && (
        <Card title="Send a test" subtitle="Save first, then send one text to your own phone">
          <div className="row">
            <TextInput
              label="Phone number"
              value={testPhone}
              onChange={(event) => setTestPhone(event.target.value)}
              placeholder="07XX XXX XXX"
            />
            <Button
              style={{ alignSelf: "flex-end" }}
              loading={busy === "test"}
              disabled={testPhone.trim().length < 9}
              onClick={() => {
                setBusy("test");
                api
                  .sendTestSms(testPhone)
                  .then((message) => toast("success", message))
                  .catch(reportError)
                  .finally(() => {
                    setBusy(null);
                    void loadOutbox();
                  });
              }}
            >
              Send test
            </Button>
          </div>
        </Card>
      )}

      <Card
        title="Outbox"
        subtitle={`The last 100 texts${waiting ? `, ${waiting} waiting` : ""}${failed ? `, ${failed} failed` : ""}`}
        actions={
          <Button
            size="sm"
            loading={busy === "send"}
            disabled={waiting + failed === 0}
            onClick={() => {
              setBusy("send");
              api
                .sendQueuedSms()
                .then((result) => {
                  toast(
                    result.skippedReason ? "info" : "success",
                    result.skippedReason ??
                      `Sent ${result.sent}. ${result.stillQueued} still waiting, ${result.failed} failed.`,
                  );
                })
                .catch(reportError)
                .finally(() => {
                  setBusy(null);
                  void loadOutbox();
                });
            }}
          >
            Send waiting texts now
          </Button>
        }
        flush
      >
        {outbox.length === 0 ? (
          <p className="muted" style={{ padding: "var(--space-4)" }}>
            No texts yet. Pass-outs send them automatically.
          </p>
        ) : (
          <div className="table-wrap" style={{ maxHeight: 420 }}>
            <table className="table table-compact">
              <thead>
                <tr>
                  <th style={{ width: 120 }}>When</th>
                  <th style={{ width: 140 }}>To</th>
                  <th>Message</th>
                  <th style={{ width: 100 }}>Status</th>
                </tr>
              </thead>
              <tbody>
                {outbox.map((row) => (
                  <tr key={row.id}>
                    <td className="muted">{formatDateTime(row.createdAt)}</td>
                    <td className="mono">{row.toPhone}</td>
                    <td style={{ whiteSpace: "normal" }}>
                      {row.body}
                      {row.lastError && row.status !== "sent" && (
                        <div className="subtle" style={{ fontSize: "var(--text-2xs)" }}>
                          {row.lastError}
                        </div>
                      )}
                    </td>
                    <td>
                      <Badge
                        tone={
                          row.status === "sent"
                            ? "success"
                            : row.status === "failed"
                              ? "danger"
                              : "warning"
                        }
                      >
                        {row.status === "queued" ? "Waiting" : row.status === "sent" ? "Sent" : "Failed"}
                      </Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// FR-B7 — the optional features panel
// ---------------------------------------------------------------------------

function FeaturesTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const refreshInstitution = useStore((state) => state.refreshInstitution);

  const [features, setFeatures] = useState<FeatureFlags | null>(null);

  useEffect(() => {
    api.getFeatures().then(setFeatures).catch(reportError);
  }, [reportError]);

  if (!features) return <Loading />;

  const entries: {
    key: keyof FeatureFlags;
    label: string;
    detail: string;
    ready: boolean;
  }[] = [
    {
      key: "studentPhotos",
      label: "Student photos",
      detail: "Adds a photo to each learner's record and to the report card.",
      ready: true,
    },
    {
      key: "examPermits",
      label: "Examination permits",
      detail:
        "Prints an examination permit for each learner (Reports & printing). Learners blocked on fees get none.",
      ready: true,
    },
    {
      key: "streams",
      label: "Streams",
      detail:
        "Up to 20 streams per class (Classes & subjects), each with its own Class Teacher and roster.",
      ready: true,
    },
    {
      key: "weeklyAssignments",
      label: "Weekly assignments",
      detail:
        "Per-subject weekly scores (Weekly assignments), summarised on the back of the report card.",
      ready: true,
    },
  ];

  return (
    <div className="stack">
      <Alert tone="info" title="Turning a feature off never deletes anything">
        Data already recorded is kept and simply hidden, so a feature can be
        switched back on mid-year without loss.
      </Alert>

      <Card title="Optional features" subtitle="All off by default" flush>
        <div style={{ padding: "var(--space-2)" }}>
          {entries.map((entry) => (
            <div
              key={entry.key}
              className="list-row"
              style={{ cursor: "default", padding: "var(--space-4)" }}
            >
              <span
                className="empty-icon"
                style={{ width: 32, height: 32, borderRadius: "var(--radius-md)" }}
              >
                <ToggleRight size={16} />
              </span>
              <span className="grow">
                <span className="row" style={{ gap: "var(--space-2)" }}>
                  <span className="list-row-title">{entry.label}</span>
                  {!entry.ready && <Badge tone="neutral">Next phase</Badge>}
                </span>
                <span className="list-row-meta" style={{ display: "block" }}>
                  {entry.detail}
                </span>
              </span>
              <Switch
                checked={features[entry.key]}
                disabled={!entry.ready}
                label={entry.label}
                onChange={(next) => {
                  setFeatures({ ...features, [entry.key]: next });
                  api
                    .setFeature(toSnake(entry.key), next)
                    .then(() => {
                      toast("success", `${entry.label} turned ${next ? "on" : "off"}.`);
                      void refreshInstitution();
                    })
                    .catch((error) => {
                      setFeatures({ ...features, [entry.key]: !next });
                      reportError(error);
                    });
                }}
              />
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

function toSnake(key: string): string {
  return key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

// ---------------------------------------------------------------------------
// FR-B10 — backup status
// ---------------------------------------------------------------------------

function BackupTab() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  // Bumped after each backup so the restore list shows the new snapshot.
  const [backupsTaken, setBackupsTaken] = useState(0);

  function reload() {
    api.backupStatus().then(setStatus).catch(reportError);
  }

  useEffect(reload, [reportError]);

  if (!status) return <Loading />;

  const stale =
    !status.lastBackupAt ||
    Date.now() - new Date(status.lastBackupAt).getTime() > 3 * 24 * 60 * 60 * 1000;

  return (
    <div className="stack">
      <Card
        title="Backups"
        subtitle="A consistent snapshot, safe to take while RM is in use"
        actions={
          <Button
            variant="primary"
            icon={<DatabaseBackup size={15} />}
            loading={busy}
            onClick={() => {
              setBusy(true);
              api
                .runBackup()
                .then((result) => {
                  toast(
                    "success",
                    result.mirrorPath
                      ? "Backed up to this computer and the second drive."
                      : "Backed up to this computer.",
                  );
                  reload();
                  setBackupsTaken((n) => n + 1);
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Back up now
          </Button>
        }
      >
        <div className="stack">
          {stale && (
            <Alert tone="warning" title="Take a backup">
              {status.lastBackupAt
                ? `The last one was ${relativeTime(status.lastBackupAt)}.`
                : "This school has never been backed up."}
            </Alert>
          )}

          <Row label="Last backup" value={formatDateTime(status.lastBackupAt)} />
          <Row label="Kept on this computer at" value={status.localBackupDir} mono />
          <Row
            label="Second drive"
            value={status.mirrorPath ?? "Not set — backups stay on this computer only"}
            mono={Boolean(status.mirrorPath)}
          />
        </div>
      </Card>

      <Card
        title="Second drive"
        subtitle="Every backup is written here as well as locally"
        footer={
          <div className="row">
            <Button
              icon={<HardDrive size={15} />}
              onClick={() => {
                openDialog({ directory: true, multiple: false })
                  .then((path) => {
                    if (typeof path !== "string") return;
                    return api.setMirrorPath(path).then(() => {
                      toast("success", "Backup drive set.");
                      reload();
                    });
                  })
                  .catch((error) => {
                    if (error instanceof ApiError) reportError(error);
                  });
              }}
            >
              Choose a folder on the drive
            </Button>
            {status.mirrorPath && (
              <Button
                variant="ghost"
                onClick={() =>
                  api
                    .setMirrorPath(null)
                    .then(() => {
                      toast("success", "Backup drive cleared.");
                      reload();
                    })
                    .catch(reportError)
                }
              >
                Clear
              </Button>
            )}
          </div>
        }
      >
        <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
          Plug in an external SSD and choose a folder on it. RM keeps the twenty
          most recent snapshots locally so a school PC never fills up. Rotating
          one drive off site at the end of each term is the cheapest protection
          there is against fire or theft.
        </p>
      </Card>

      <RestoreCard key={backupsTaken} />

      <Card title="Cloud backup">
        <div className="row-between">
          <span>
            <span style={{ fontWeight: 500 }}>Encrypted cloud backup</span>
            <span className="field-hint" style={{ display: "block" }}>
              Built and switched off at launch. When it is enabled, every school
              gets it with no reinstall, encrypted with a key the developer does
              not hold.
            </span>
          </span>
          <Badge tone="neutral">Coming soon</Badge>
        </div>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Restoring a backup (FR-E4, scenario 1)
// ---------------------------------------------------------------------------

function RestoreCard() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const signOut = useStore((state) => state.signOut);

  const [files, setFiles] = useState<BackupFile[] | null>(null);
  const [preview, setPreview] = useState<BackupSummary | null>(null);
  const [restoring, setRestoring] = useState(false);

  useEffect(() => {
    api.listBackups().then(setFiles).catch(reportError);
  }, [reportError]);

  function inspect(path: string) {
    api.inspectBackup(path).then(setPreview).catch(reportError);
  }

  function chooseFile() {
    openDialog({
      multiple: false,
      filters: [{ name: "Phantom School Manager backup", extensions: ["rmdb"] }],
    })
      .then((path) => {
        if (typeof path === "string") inspect(path);
      })
      .catch((error) => {
        if (error instanceof ApiError) reportError(error);
      });
  }

  function restore(path: string) {
    setRestoring(true);
    api
      .restoreBackup(path)
      .then(() => {
        setPreview(null);
        toast("success", "Backup restored. Sign in again to continue.");
        return signOut();
      })
      .catch(reportError)
      .finally(() => setRestoring(false));
  }

  return (
    <Card
      title="Restore a backup"
      subtitle="Replace the school's data with an earlier snapshot"
      actions={
        <Button icon={<FolderOpen size={15} />} onClick={chooseFile}>
          Choose a file
        </Button>
      }
    >
      {!files ? (
        <Loading />
      ) : files.length === 0 ? (
        <p className="muted" style={{ fontSize: "var(--text-sm)" }}>
          No backups yet. Take one above, or choose a backup file from another drive.
        </p>
      ) : (
        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th>Taken</th>
                <th>Where</th>
                <th className="num">Size</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {files.slice(0, 15).map((file) => (
                <tr key={file.path}>
                  <td>
                    {formatDateTime(file.modifiedAt)}
                    {file.fileName.includes("before-restore") && (
                      <div className="subtle" style={{ fontSize: "var(--text-xs)" }}>
                        Saved before a restore
                      </div>
                    )}
                  </td>
                  <td>
                    <Badge tone={file.location === "mirror" ? "accent" : "neutral"}>
                      {file.location === "mirror" ? "Second drive" : "This computer"}
                    </Badge>
                  </td>
                  <td className="num">{formatBytes(file.bytes)}</td>
                  <td style={{ textAlign: "right" }}>
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<History size={14} />}
                      onClick={() => inspect(file.path)}
                    >
                      Restore…
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <Modal
        open={preview !== null}
        title="Restore this backup?"
        description="Everything entered since this backup was taken will be replaced."
        onClose={() => !restoring && setPreview(null)}
        footer={
          <div className="row">
            <Button variant="ghost" disabled={restoring} onClick={() => setPreview(null)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              loading={restoring}
              disabled={!preview?.compatible}
              onClick={() => preview && restore(preview.path)}
            >
              Restore
            </Button>
          </div>
        }
      >
        {preview && (
          <div className="stack">
            {!preview.compatible && (
              <Alert tone="danger" title="Made by a newer version of Phantom School Manager">
                Update RM on this computer before restoring this backup.
              </Alert>
            )}
            <Row label="School" value={preview.institutionName ?? "Not set up yet"} />
            <Row label="Learners" value={String(preview.learners)} />
            <Row label="Marks" value={String(preview.marks)} />
            <Row label="Last activity" value={formatDateTime(preview.lastActivityAt)} />
            <Row label="File" value={preview.path} mono />
            <Alert tone="info" title="Nothing is lost">
              The data on this computer now is saved as a backup first, so this
              restore can itself be undone. Everyone is signed out afterwards.
            </Alert>
          </div>
        )}
      </Modal>
    </Card>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function Row({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="row-between" style={{ alignItems: "flex-start" }}>
      <span className="field-label" style={{ flex: "none", width: 190 }}>
        {label}
      </span>
      <span
        className={cx("grow selectable", mono && "mono")}
        style={{ fontSize: "var(--text-sm)", textAlign: "right", wordBreak: "break-all" }}
      >
        {value}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The signed-in person's own account
// ---------------------------------------------------------------------------

function AccountTab() {
  const session = useStore((state) => state.session);
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const refreshSession = useStore((state) => state.refreshSession);

  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);

  if (!session) return null;

  const mismatch = confirm.length > 0 && confirm !== next;

  return (
    <div className="stack">
      {session.mustChangePassword && (
        <Alert tone="warning" title="Choose your own password">
          You are still using the password you were given. Change it now so
          nobody else can sign in as you.
        </Alert>
      )}

      <Card
        title="Change my password"
        subtitle={`Signed in as ${session.username}`}
        footer={
          <div className="row-between">
            <span className="field-hint">
              At least 10 characters. A short phrase beats a puzzle.
            </span>
            <Button
              variant="primary"
              icon={<ShieldCheck size={15} />}
              loading={busy}
              disabled={!current || next.length < 10 || mismatch || !confirm}
              onClick={() => {
                setBusy(true);
                api
                  .changePassword(current, next)
                  .then(() => {
                    toast("success", "Your password has been changed.");
                    setCurrent("");
                    setNext("");
                    setConfirm("");
                    void refreshSession();
                  })
                  .catch(reportError)
                  .finally(() => setBusy(false));
              }}
            >
              Change password
            </Button>
          </div>
        }
      >
        <div className="grid-form">
          <TextInput
            label="Current password"
            type="password"
            className="span-2"
            value={current}
            onChange={(event) => setCurrent(event.target.value)}
            autoComplete="current-password"
          />
          <TextInput
            label="New password"
            type="password"
            value={next}
            onChange={(event) => setNext(event.target.value)}
            autoComplete="new-password"
          />
          <TextInput
            label="Confirm new password"
            type="password"
            value={confirm}
            onChange={(event) => setConfirm(event.target.value)}
            autoComplete="new-password"
            error={mismatch ? "The two passwords do not match." : null}
          />
        </div>
      </Card>

      <Card title="This session">
        <div className="stack">
          <Row label="Name" value={session.fullName} />
          <Row label="Username" value={session.username} mono />
          <Row
            label="Role"
            value={session.role === "school_admin" ? "School Admin" : "Teacher"}
          />
          <Row label="Signed in" value={formatDateTime(session.signedInAt)} />
        </div>
        <p className="field-hint" style={{ marginTop: "var(--space-4)" }}>
          RM signs you out automatically after 30 minutes without activity, so a
          machine left unattended in a shared office does not stay open.
        </p>
      </Card>
    </div>
  );
}
