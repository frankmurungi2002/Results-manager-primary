/**
 * The grading library and the Custom Band Builder (SRS 4.2).
 *
 * Per SRS 16.2, only the two systems a Ugandan primary school actually uses
 * ship with real thresholds, plus Percentage Only. The rest of the worldwide
 * library is not seeded with guessed boundaries — the band builder is how a
 * school gets anything else, and how it corrects a preset before going live.
 */

import { useCallback, useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { GradingSystem } from "../lib/types";
import { useStore } from "../state/store";
import {
  Alert,
  Badge,
  Button,
  Card,
  Loading,
  Modal,
  SelectInput,
  TextInput,
  cx,
} from "../components/ui";

interface BandDraft {
  label: string;
  lowerBound: string;
  upperBound: string;
  points: string;
  remark: string;
}

export function GradingScreen() {
  const reportError = useStore((state) => state.reportError);

  const [systems, setSystems] = useState<GradingSystem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [loading, setLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorSeed, setEditorSeed] = useState<GradingSystem | null>(null);

  const reload = useCallback(async () => {
    try {
      const loaded = await api.listGradingSystems();
      setSystems(loaded);
      setSelectedId((current) =>
        loaded.some((entry) => entry.id === current) ? current : (loaded[0]?.id ?? ""),
      );
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const selected = systems.find((entry) => entry.id === selectedId);

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
            <h1 className="page-title">Grading systems</h1>
            <p className="page-description">
              A mark is converted to a percentage of its subject's maximum
              before a band is applied, so a subject marked out of 40 grades
              exactly like one marked out of 100.
            </p>
          </div>
          <div className="page-actions">
            <Button
              variant="primary"
              icon={<Plus size={15} />}
              onClick={() => {
                setEditorSeed(null);
                setEditorOpen(true);
              }}
            >
              New system
            </Button>
          </div>
        </div>

        <div className="split">
          <Card className="list-panel" title="Systems" flush>
            <div className="list-scroll">
              {systems.map((system) => (
                <button
                  key={system.id}
                  className={cx("list-row", system.id === selectedId && "is-active")}
                  onClick={() => setSelectedId(system.id)}
                >
                  <span className="grow">
                    <span className="list-row-title">{system.name}</span>
                    <span className="list-row-meta" style={{ display: "block" }}>
                      {system.bands.length} band{system.bands.length === 1 ? "" : "s"}
                    </span>
                  </span>
                  {system.is_builtin && <Badge tone="neutral">Built in</Badge>}
                </button>
              ))}
            </div>
          </Card>

          {selected && (
            <div className="stack">
              <Card
                title={selected.name}
                subtitle={selected.description ?? undefined}
                actions={
                  <>
                    <Button
                      size="sm"
                      onClick={() => {
                        setEditorSeed(selected);
                        setEditorOpen(true);
                      }}
                    >
                      {selected.is_editable ? "Edit" : "Duplicate & edit"}
                    </Button>
                  </>
                }
                flush
              >
                <div className="table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th style={{ width: 110 }}>Grade</th>
                        <th style={{ width: 160 }}>Percentage</th>
                        <th style={{ width: 100 }} className="num">
                          Points
                        </th>
                        <th>Remark</th>
                      </tr>
                    </thead>
                    <tbody>
                      {selected.bands.map((band) => (
                        <tr key={band.id}>
                          <td style={{ fontWeight: 600 }}>{band.label}</td>
                          <td className="mono muted">
                            {band.lower_bound} – {band.upper_bound}
                          </td>
                          <td className="num">{band.points ?? "—"}</td>
                          <td className="muted">{band.remark ?? "—"}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Card>

              {selected.is_builtin && (
                <Alert tone="warning" title="Check the thresholds before you go live">
                  Examination boards revise their boundaries from time to time.
                  Compare these against the current circular, and if they differ,
                  duplicate this system and correct it — Phantom School Manager will not stop you
                  printing with the wrong ones.
                </Alert>
              )}
            </div>
          )}
        </div>
      </div>

      <BandEditor
        open={editorOpen}
        seed={editorSeed}
        onClose={() => setEditorOpen(false)}
        onSaved={() => {
          setEditorOpen(false);
          void reload();
        }}
      />
    </div>
  );
}

function BandEditor({
  open,
  seed,
  onClose,
  onSaved,
}: {
  open: boolean;
  seed: GradingSystem | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [name, setName] = useState("");
  const [kind, setKind] = useState("numeric");
  const [bands, setBands] = useState<BandDraft[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    if (seed) {
      setName(seed.is_editable ? seed.name : `${seed.name} (copy)`);
      setKind(seed.kind);
      setBands(
        seed.bands.map((band) => ({
          label: band.label,
          lowerBound: String(band.lower_bound),
          upperBound: String(band.upper_bound),
          points: band.points === null ? "" : String(band.points),
          remark: band.remark ?? "",
        })),
      );
    } else {
      setName("");
      setKind("numeric");
      setBands([
        { label: "A", lowerBound: "80", upperBound: "100", points: "1", remark: "" },
        { label: "B", lowerBound: "60", upperBound: "79", points: "2", remark: "" },
        { label: "C", lowerBound: "40", upperBound: "59", points: "3", remark: "" },
        { label: "F", lowerBound: "0", upperBound: "39", points: "4", remark: "" },
      ]);
    }
  }, [open, seed]);

  // The same coverage check the backend enforces, run as you type so a gap is
  // visible before saving rather than as a rejection afterwards.
  const problem = checkCoverage(bands);

  return (
    <Modal
      open={open}
      wide
      title={seed?.is_editable ? "Edit grading system" : "New grading system"}
      description="Bands must cover 0 to 100 with no gap and no overlap, so every possible mark lands in exactly one grade."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!name.trim() || problem !== null}
            onClick={() => {
              setBusy(true);
              api
                .saveGradingSystem({
                  id: seed?.is_editable ? seed.id : undefined,
                  name,
                  kind,
                  bands: bands.map((band) => ({
                    label: band.label,
                    lowerBound: Number(band.lowerBound),
                    upperBound: Number(band.upperBound),
                    points: band.points.trim() === "" ? null : Number(band.points),
                    remark: band.remark.trim() || null,
                  })),
                })
                .then(() => {
                  toast("success", "Grading system saved.");
                  onSaved();
                })
                .catch(reportError)
                .finally(() => setBusy(false));
            }}
          >
            Save system
          </Button>
        </>
      }
    >
      <div className="stack">
        <div className="grid-form">
          <TextInput
            label="Name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="e.g. Lower Primary Descriptive"
          />
          <SelectInput
            label="Kind"
            value={kind}
            onChange={(event) => setKind(event.target.value)}
          >
            <option value="numeric">Numeric (D1, C3, F9…)</option>
            <option value="letter">Letter (A, B, C…)</option>
            <option value="descriptive">Descriptive (Excellent, Good…)</option>
            <option value="percentage">Percentage only</option>
          </SelectInput>
        </div>

        {problem && (
          <Alert tone="danger" title="These bands are not usable yet">
            {problem}
          </Alert>
        )}

        <div className="table-wrap">
          <table className="table table-compact">
            <thead>
              <tr>
                <th style={{ width: 130 }}>Grade</th>
                <th style={{ width: 90 }} className="num">
                  From %
                </th>
                <th style={{ width: 90 }} className="num">
                  To %
                </th>
                <th style={{ width: 90 }} className="num">
                  Points
                </th>
                <th>Remark</th>
                <th style={{ width: 44 }} />
              </tr>
            </thead>
            <tbody>
              {bands.map((band, index) => (
                <tr key={index}>
                  {(["label", "lowerBound", "upperBound", "points", "remark"] as const).map(
                    (key) => (
                      <td key={key} className={key === "label" || key === "remark" ? "" : "num"}>
                        <input
                          className="input"
                          style={{
                            height: 30,
                            textAlign: key === "label" || key === "remark" ? "left" : "right",
                          }}
                          value={band[key]}
                          onChange={(event) =>
                            setBands((current) =>
                              current.map((entry, i) =>
                                i === index ? { ...entry, [key]: event.target.value } : entry,
                              ),
                            )
                          }
                        />
                      </td>
                    ),
                  )}
                  <td>
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<Trash2 size={14} />}
                      title="Remove band"
                      onClick={() =>
                        setBands((current) => current.filter((_, i) => i !== index))
                      }
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <Button
          size="sm"
          icon={<Plus size={14} />}
          onClick={() =>
            setBands((current) => [
              ...current,
              { label: "", lowerBound: "0", upperBound: "0", points: "", remark: "" },
            ])
          }
        >
          Add a band
        </Button>
      </div>
    </Modal>
  );
}

/** Mirrors `GradingSystem::validate` in the backend. */
function checkCoverage(bands: BandDraft[]): string | null {
  if (bands.length === 0) return "Add at least one band.";

  const parsed = bands.map((band) => ({
    label: band.label.trim(),
    lower: Number(band.lowerBound),
    upper: Number(band.upperBound),
  }));

  for (const band of parsed) {
    if (!band.label) return "Every band needs a grade label.";
    if (Number.isNaN(band.lower) || Number.isNaN(band.upper))
      return `Band “${band.label}” has a percentage that is not a number.`;
    if (band.lower > band.upper)
      return `Band “${band.label}” starts above where it ends.`;
    if (band.lower < 0 || band.upper > 100)
      return `Band “${band.label}” must sit between 0 and 100.`;
  }

  const sorted = [...parsed].sort((a, b) => a.lower - b.lower);
  if (sorted[0]!.lower > 0)
    return `Nothing covers 0 to ${sorted[0]!.lower}. Every mark must land in a band.`;

  for (let index = 0; index < sorted.length - 1; index += 1) {
    const left = sorted[index]!;
    const right = sorted[index + 1]!;
    if (right.lower <= left.upper)
      return `Bands “${left.label}” and “${right.label}” overlap.`;
    if (right.lower - left.upper > 1.000001)
      return `Nothing covers the marks between “${left.label}” and “${right.label}”.`;
  }

  const top = sorted[sorted.length - 1]!;
  if (top.upper < 100) return `Nothing covers ${top.upper} to 100.`;

  return null;
}
