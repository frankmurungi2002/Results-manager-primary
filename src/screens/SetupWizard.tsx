/**
 * First-run setup.
 *
 * FR-B2 (institution name compulsory, logo optional), FR-B3 (calendar with
 * Ugandan defaults), FR-B5 (classes and subject catalogue), FR-B4 (the first
 * School Admin). SRS 14.4 sets the bar: a headteacher with no IT background
 * should get through this without a hired technician, so every step arrives
 * pre-filled with something sensible and nothing is asked twice.
 */

import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, ArrowRight, Check, PartyPopper } from "lucide-react";

import { ApiError, api } from "../lib/api";
import type { DefaultExam, SetupDefaults } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Button,
  Card,
  Loading,
  SelectInput,
  TextInput,
  cx,
} from "../components/ui";

const STEPS = ["School", "Classes", "Calendar", "Your account"] as const;

const ACCENTS = [
  { name: "Indigo", value: "#4F63D2" },
  { name: "Teal", value: "#0E8577" },
  { name: "Maroon", value: "#9B2C3A" },
  { name: "Forest", value: "#2A6B3F" },
  { name: "Navy", value: "#1F3A70" },
  { name: "Plum", value: "#6B3A82" },
];

export function SetupWizard() {
  const markProvisioned = useStore((state) => state.markProvisioned);
  const toast = useStore((state) => state.toast);

  const [defaults, setDefaults] = useState<SetupDefaults | null>(null);
  const [step, setStep] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<{ username: string } | null>(null);

  // --- Step 1: the school ---
  const [institutionName, setInstitutionName] = useState("");
  const [motto, setMotto] = useState("");
  const [address, setAddress] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");
  const [accentColor, setAccentColor] = useState(ACCENTS[0]!.value);

  // --- Step 2: classes ---
  const [classCodes, setClassCodes] = useState<string[]>([]);
  const [regPattern, setRegPattern] = useState("{YEAR}/{SEQ:4}");

  // --- Step 3: calendar ---
  const [yearLabel, setYearLabel] = useState("");
  const [termCount, setTermCount] = useState(3);
  const [exams, setExams] = useState<DefaultExam[]>([]);

  // --- Step 4: the first School Admin ---
  const [adminName, setAdminName] = useState("");
  const [adminUsername, setAdminUsername] = useState("");
  const [adminEmail, setAdminEmail] = useState("");
  const [adminPassword, setAdminPassword] = useState("");
  const [adminConfirm, setAdminConfirm] = useState("");

  useEffect(() => {
    api
      .setupDefaults()
      .then((loaded) => {
        setDefaults(loaded);
        // Everything a Ugandan nursery-and-primary school runs, pre-ticked.
        setClassCodes(loaded.classes.map((entry) => entry.code));
        setYearLabel(loaded.suggestedYearLabel);
        setExams(loaded.exams);
        setRegPattern(loaded.regNumberPattern);
      })
      .catch(() => setError("Could not load the setup defaults."));
  }, []);

  const nurseryClasses = useMemo(
    () => defaults?.classes.filter((entry) => entry.levelKind === "nursery") ?? [],
    [defaults],
  );
  const primaryClasses = useMemo(
    () => defaults?.classes.filter((entry) => entry.levelKind === "primary") ?? [],
    [defaults],
  );

  const subjectPreview = useMemo(() => {
    if (!defaults) return [];
    const levels = new Set(
      defaults.classes
        .filter((entry) => classCodes.includes(entry.code))
        .map((entry) => entry.levelKind),
    );
    return defaults.subjects.filter((subject) =>
      subject.levels.some((level) => levels.has(level as "nursery" | "primary")),
    );
  }, [defaults, classCodes]);

  if (!defaults) {
    return (
      <div className="auth-panel">
        <Loading label="Preparing setup" />
      </div>
    );
  }

  if (done) {
    return (
      <div className="auth-panel">
        <div className="auth-form" style={{ textAlign: "center", alignItems: "center" }}>
          <div className="empty-icon" style={{ width: 52, height: 52, color: "var(--success)" }}>
            <PartyPopper size={24} />
          </div>
          <h2 className="auth-title">{institutionName} is ready</h2>
          <p className="auth-note">
            Sign in as <strong>{done.username}</strong> with the password you
            just chose. Your next steps are to add your teachers, then your
            learners.
          </p>
          <Button
            variant="primary"
            size="lg"
            block
            onClick={() => {
              markProvisioned();
              window.location.reload();
            }}
          >
            Go to sign in
          </Button>
        </div>
      </div>
    );
  }

  const stepValid = validateStep();

  function validateStep(): string | null {
    switch (step) {
      case 0:
        return institutionName.trim().length < 2
          ? "Enter the school's name to continue."
          : null;
      case 1:
        return classCodes.length === 0 ? "Choose at least one class." : null;
      case 2:
        if (!yearLabel.trim()) return "Enter the academic year.";
        if (exams.length === 0) return "Add at least one examination.";
        if (exams.filter((exam) => exam.isFinal).length !== 1)
          return "Mark exactly one examination as the end-of-term set.";
        return null;
      case 3:
        if (adminName.trim().length < 2) return "Enter your full name.";
        if (adminUsername.trim().length < 3)
          return "Choose a username of at least 3 characters.";
        if (adminPassword.length < 10)
          return "Your password must be at least 10 characters long.";
        if (adminPassword !== adminConfirm) return "The two passwords do not match.";
        return null;
      default:
        return null;
    }
  }

  async function finish() {
    setError(null);
    setBusy(true);
    try {
      const result = await api.completeSetup({
        institutionName: institutionName.trim(),
        motto: motto.trim() || null,
        address: address.trim() || null,
        phone: phone.trim() || null,
        email: email.trim() || null,
        accentColor,
        regNumberPattern: regPattern.trim(),
        adminFullName: adminName.trim(),
        adminUsername: adminUsername.trim(),
        adminPassword,
        adminEmail: adminEmail.trim() || null,
        academicYearLabel: yearLabel.trim(),
        classCodes,
        terms: defaults!.terms
          .slice(0, termCount)
          .map((name) => ({ name, startDate: null, endDate: null })),
        exams: exams.map((exam) => ({
          code: exam.code,
          name: exam.name,
          weight: exam.weight,
          isFinal: exam.isFinal,
        })),
      });
      toast("success", `${result.institutionName} is set up.`);
      setDone({ username: result.adminUsername });
    } catch (caught) {
      setError(
        caught instanceof ApiError ? caught.message : "Setup could not be completed.",
      );
    } finally {
      setBusy(false);
    }
  }

  function toggleClass(code: string) {
    setClassCodes((current) =>
      current.includes(code)
        ? current.filter((entry) => entry !== code)
        : [...current, code],
    );
  }

  return (
    <div style={{ height: "100%", overflowY: "auto", background: "var(--bg-canvas)" }}>
      <div className="wizard">
        <div>
          <h1 className="page-title">Set up Phantom School Manager</h1>
          <p className="page-description">
            Four short steps. Everything here can be changed later from
            Settings — nothing you choose now is permanent.
          </p>
        </div>

        <div className="wizard-steps">
          {STEPS.map((label, index) => (
            <div key={label} style={{ display: "contents" }}>
              <div
                className={cx(
                  "wizard-step",
                  index === step && "is-active",
                  index < step && "is-done",
                )}
              >
                <span className="wizard-step-dot">
                  {index < step ? <Check size={12} /> : index + 1}
                </span>
                <span>{label}</span>
              </div>
              {index < STEPS.length - 1 && <div className="wizard-rule" />}
            </div>
          ))}
        </div>

        {error && <Alert tone="danger">{error}</Alert>}

        {step === 0 && (
          <Card
            title="About your school"
            subtitle="The name appears on every document Phantom School Manager prints. A logo is optional."
          >
            <div className="grid-form">
              <TextInput
                label="School name"
                className="span-2"
                value={institutionName}
                onChange={(event) => setInstitutionName(event.target.value)}
                placeholder="e.g. MB Primary School"
                autoFocus
              />
              <TextInput
                label="Motto (optional)"
                value={motto}
                onChange={(event) => setMotto(event.target.value)}
                placeholder="e.g. Knowledge is Light"
              />
              <TextInput
                label="Telephone (optional)"
                value={phone}
                onChange={(event) => setPhone(event.target.value)}
                placeholder="e.g. 0772 000 000"
              />
              <TextInput
                label="Address (optional)"
                value={address}
                onChange={(event) => setAddress(event.target.value)}
                placeholder="e.g. P.O. Box 123, Wakiso"
              />
              <TextInput
                label="Email (optional)"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </div>

            <div className="field" style={{ marginTop: "var(--space-5)" }}>
              <span className="field-label">Accent colour</span>
              <div className="row" style={{ flexWrap: "wrap" }}>
                {ACCENTS.map((accent) => (
                  <button
                    key={accent.value}
                    className={cx("pick", accentColor === accent.value && "is-selected")}
                    style={{ width: 132 }}
                    onClick={() => setAccentColor(accent.value)}
                  >
                    <span
                      style={{
                        width: 16,
                        height: 16,
                        borderRadius: 4,
                        background: accent.value,
                        flex: "none",
                      }}
                    />
                    {accent.name}
                  </button>
                ))}
              </div>
              <span className="field-hint">
                Used throughout the application. You can change it at any time.
              </span>
            </div>
          </Card>
        )}

        {step === 1 && (
          <div className="stack">
            <Card
              title="Which classes does your school run?"
              subtitle="Untick anything you do not teach. You can add classes later."
            >
              {nurseryClasses.length > 0 && (
                <>
                  <div className="field-label" style={{ marginBottom: "var(--space-2)" }}>
                    Nursery
                  </div>
                  <div className="pick-grid" style={{ marginBottom: "var(--space-5)" }}>
                    {nurseryClasses.map((entry) => (
                      <ClassPick
                        key={entry.code}
                        label={entry.name}
                        selected={classCodes.includes(entry.code)}
                        onToggle={() => toggleClass(entry.code)}
                      />
                    ))}
                  </div>
                </>
              )}

              <div className="field-label" style={{ marginBottom: "var(--space-2)" }}>
                Primary
              </div>
              <div className="pick-grid">
                {primaryClasses.map((entry) => (
                  <ClassPick
                    key={entry.code}
                    label={entry.name}
                    selected={classCodes.includes(entry.code)}
                    onToggle={() => toggleClass(entry.code)}
                  />
                ))}
              </div>
            </Card>

            <Card
              title="Registration numbers"
              subtitle="Phantom School Manager makes sure every learner's number is unique across the school."
            >
              <div className="grid-form">
                <TextInput
                  label="Format"
                  value={regPattern}
                  onChange={(event) => setRegPattern(event.target.value)}
                  hint="{YEAR} is the year, {SEQ:4} is a running number padded to 4 digits."
                />
                <div className="field">
                  <span className="field-label">Example</span>
                  <div
                    className="mono"
                    style={{
                      height: 34,
                      display: "flex",
                      alignItems: "center",
                      padding: "0 var(--space-3)",
                      background: "var(--bg-sunken)",
                      borderRadius: "var(--radius-md)",
                      border: "1px solid var(--border-subtle)",
                    }}
                  >
                    {previewRegNumber(regPattern)}
                  </div>
                </div>
              </div>
            </Card>

            {subjectPreview.length > 0 && (
              <Alert tone="info" title={`${subjectPreview.length} subjects will be created`}>
                {subjectPreview.map((subject) => subject.name).join(", ")}. Each
                class gets the ones that apply to it, and you can add, rename or
                remove any of them afterwards.
              </Alert>
            )}
          </div>
        )}

        {step === 2 && (
          <div className="stack">
            <Card
              title="Academic year"
              subtitle="Ugandan defaults are filled in. Adjust them if your school differs."
            >
              <div className="grid-form">
                <TextInput
                  label="Academic year"
                  value={yearLabel}
                  onChange={(event) => setYearLabel(event.target.value)}
                  placeholder="e.g. 2026"
                />
                <SelectInput
                  label="Terms per year"
                  value={String(termCount)}
                  onChange={(event) => setTermCount(Number(event.target.value))}
                >
                  <option value="1">1 term</option>
                  <option value="2">2 terms</option>
                  <option value="3">3 terms</option>
                </SelectInput>
              </div>
            </Card>

            <Card
              title="Examinations each term"
              subtitle="The weights decide how the term mark on a report card is worked out."
            >
              <div className="table-wrap">
                <table className="table table-compact">
                  <thead>
                    <tr>
                      <th style={{ width: 90 }}>Code</th>
                      <th>Name</th>
                      <th style={{ width: 120 }} className="num">
                        Weight
                      </th>
                      <th style={{ width: 130 }} className="center">
                        End of term
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {exams.map((exam, index) => (
                      <tr key={index}>
                        <td className="mono">{exam.code}</td>
                        <td>
                          <input
                            className="input"
                            value={exam.name}
                            onChange={(event) =>
                              setExams((current) =>
                                current.map((entry, i) =>
                                  i === index
                                    ? { ...entry, name: event.target.value }
                                    : entry,
                                ),
                              )
                            }
                          />
                        </td>
                        <td className="num">
                          <input
                            className="input"
                            type="number"
                            min={0}
                            max={1}
                            step={0.05}
                            value={exam.weight}
                            style={{ textAlign: "right" }}
                            onChange={(event) =>
                              setExams((current) =>
                                current.map((entry, i) =>
                                  i === index
                                    ? { ...entry, weight: Number(event.target.value) }
                                    : entry,
                                ),
                              )
                            }
                          />
                        </td>
                        <td className="center">
                          <input
                            type="radio"
                            name="final-exam"
                            checked={exam.isFinal}
                            onChange={() =>
                              setExams((current) =>
                                current.map((entry, i) => ({
                                  ...entry,
                                  isFinal: i === index,
                                })),
                              )
                            }
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              <p className="field-hint" style={{ marginTop: "var(--space-3)" }}>
                Weights total {exams.reduce((sum, exam) => sum + exam.weight, 0).toFixed(2)}.
                They do not have to add up to 1 — Phantom School Manager divides by whatever is
                actually entered, so a missing exam never drags a mark down.
              </p>
            </Card>
          </div>
        )}

        {step === 3 && (
          <Card
            title="Your School Admin account"
            subtitle="This is the account you will use to run the school day to day."
          >
            <div className="grid-form">
              <TextInput
                label="Your full name"
                value={adminName}
                onChange={(event) => setAdminName(event.target.value)}
                autoFocus
              />
              <TextInput
                label="Username"
                value={adminUsername}
                onChange={(event) => setAdminUsername(event.target.value)}
                spellCheck={false}
                hint="Letters, numbers, dots, dashes and underscores."
              />
              <TextInput
                label="Email (optional)"
                type="email"
                className="span-2"
                value={adminEmail}
                onChange={(event) => setAdminEmail(event.target.value)}
              />
              <TextInput
                label="Password"
                type="password"
                value={adminPassword}
                onChange={(event) => setAdminPassword(event.target.value)}
                hint="At least 10 characters. A short phrase you will remember beats a puzzle."
              />
              <TextInput
                label="Confirm password"
                type="password"
                value={adminConfirm}
                onChange={(event) => setAdminConfirm(event.target.value)}
                error={
                  adminConfirm && adminConfirm !== adminPassword
                    ? "The two passwords do not match."
                    : null
                }
              />
            </div>

            <Alert tone="warning" title="There is no way to recover this password">
              Phantom School Manager stores no copy of it and works offline, so nobody — not even
              the developer — can reset it for you. Write it down and keep it
              somewhere safe. You can add two more School Admins later.
            </Alert>
          </Card>
        )}

        <div className="row-between">
          <Button
            icon={<ArrowLeft size={15} />}
            onClick={() => setStep((current) => Math.max(0, current - 1))}
            disabled={step === 0 || busy}
          >
            Back
          </Button>

          <div className="row">
            {stepValid && <span className="field-hint">{stepValid}</span>}
            {step < STEPS.length - 1 ? (
              <Button
                variant="primary"
                onClick={() => setStep((current) => current + 1)}
                disabled={Boolean(stepValid)}
              >
                Continue
                <ArrowRight size={15} />
              </Button>
            ) : (
              <Button
                variant="primary"
                loading={busy}
                disabled={Boolean(stepValid)}
                onClick={() => void finish()}
              >
                Finish setup
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function ClassPick({
  label,
  selected,
  onToggle,
}: {
  label: string;
  selected: boolean;
  onToggle: () => void;
}) {
  return (
    <button className={cx("pick", selected && "is-selected")} onClick={onToggle}>
      <span className="pick-check">{selected && <Check size={11} strokeWidth={3} />}</span>
      {label}
    </button>
  );
}

function previewRegNumber(pattern: string): string {
  const year = new Date().getFullYear();
  return pattern
    .replace("{YEAR}", String(year))
    .replace("{YY}", String(year).slice(2))
    .replace(/\{SEQ:(\d+)\}/, (_, width: string) => "1".padStart(Number(width), "0"))
    .replace("{SEQ}", "1");
}
