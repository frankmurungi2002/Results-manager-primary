/**
 * FR-B2 (institution identity and logo), FR-B7 (the optional features panel),
 * FR-B9 (theme), FR-B10 (backup status) and the account's own password.
 */

import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  DatabaseBackup,
  HardDrive,
  Image as ImageIcon,
  Palette,
  ShieldCheck,
  ToggleRight,
  Trash2,
} from "lucide-react";

import { ApiError, api } from "../lib/api";
import type { BackupStatus, FeatureFlags, Institution } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  Loading,
  Segmented,
  Switch,
  TextInput,
  cx,
  formatDateTime,
  relativeTime,
} from "../components/ui";

type Tab = "school" | "features" | "backup" | "account";

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

      // Read through the webview's own fetch of the file URL is not available
      // under the app's CSP, so the file is handed to the backend by path in a
      // later build. For now, guide the user rather than failing silently.
      toast(
        "info",
        "Logo upload arrives in the next build. The header already leaves room for it.",
      );
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

      <Card title="Appearance" subtitle="FR-B9 — applies to this computer only">
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
      detail: "Prints a permit per learner, respecting the fees block.",
      ready: false,
    },
    {
      key: "streams",
      label: "Streams",
      detail:
        "Up to 20 streams per class, each with its own Class Teacher and roster.",
      ready: false,
    },
    {
      key: "weeklyAssignments",
      label: "Weekly assignments",
      detail:
        "Per-subject weekly scores, summarised on the back of the report card.",
      ready: false,
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
