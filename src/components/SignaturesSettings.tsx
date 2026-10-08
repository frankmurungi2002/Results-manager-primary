/**
 * Signatures and the school stamp: photographed once, printed on every report
 * card and permit. The backend cleans the photo into ink on a transparent
 * background; this panel shows it on a signature line before it is saved, so
 * what the office approves is exactly what will print.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Camera, PenLine, Stamp as StampIcon, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { SignatureKind, SignatureOverview } from "../lib/types";
import { useStore } from "../state/store";
import { Alert, Badge, Button, Card, Loading, Modal, Switch } from "./ui";

interface Pending {
  kind: SignatureKind;
  userId: string | null;
  title: string;
  png: number[];
  url: string;
}

function toUrl(png: number[]): string {
  return URL.createObjectURL(new Blob([new Uint8Array(png)], { type: "image/png" }));
}

export function SignaturesSettings() {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);
  const [overview, setOverview] = useState<SignatureOverview | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);

  const load = useCallback(() => {
    api.getSignatures().then(setOverview).catch(reportError);
  }, [reportError]);

  useEffect(load, [load]);

  const pick = async (kind: SignatureKind, userId: string | null, title: string) => {
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Photo", extensions: ["jpg", "jpeg", "png"] }],
      });
      if (typeof path !== "string") return;
      setBusy(`${kind}:${userId ?? ""}`);
      // Rust reads the file, checks it and cleans it; nothing is stored yet.
      const png = await api.prepareSignatureImage(path, kind);
      setPending({ kind, userId, title, png, url: toUrl(png) });
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(null);
    }
  };

  const save = () => {
    if (!pending) return;
    setBusy("save");
    api
      .saveSignature(pending.kind, pending.userId, pending.png)
      .then(() => {
        toast("success", `${pending.title} saved.`);
        URL.revokeObjectURL(pending.url);
        setPending(null);
        load();
      })
      .catch(reportError)
      .finally(() => setBusy(null));
  };

  const remove = (kind: SignatureKind, userId: string | null, title: string) => {
    api
      .removeSignature(kind, userId)
      .then(() => {
        toast("success", `${title} removed. It no longer prints.`);
        load();
      })
      .catch(reportError);
  };

  if (!overview) return <Loading label="Loading signatures" />;

  const slot = (
    kind: SignatureKind,
    userId: string | null,
    title: string,
    current: string | null,
    stamp = false,
  ) => (
    <div className="sig-slot" key={`${kind}:${userId ?? ""}`}>
      <div className="sig-slot-head">
        <div className="sig-slot-title">{title}</div>
        {current ? <Badge tone="success">On file</Badge> : <Badge>None</Badge>}
      </div>
      <div className={stamp ? "sig-preview sig-preview-stamp" : "sig-preview"}>
        {current ? <img src={current} alt="" /> : <span className="subtle">Nothing yet</span>}
      </div>
      <div className="row">
        <Button
          size="sm"
          icon={<Camera size={14} />}
          loading={busy === `${kind}:${userId ?? ""}`}
          onClick={() => void pick(kind, userId, title)}
        >
          {current ? "Replace" : "Add from a photo"}
        </Button>
        {current && (
          <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => remove(kind, userId, title)}>
            Remove
          </Button>
        )}
      </div>
    </div>
  );

  return (
    <div className="stack" style={{ marginTop: "var(--space-5)" }}>
      <Alert tone="info" title="How to photograph a signature or the stamp">
        Sign, or stamp, on a plain white sheet. Photograph it with a phone in good light, holding the phone
        straight above the paper. Shadows and grey paper are removed automatically; only the ink is kept.
      </Alert>

      {overview.canManageSchool && (
        <Card
          title="The school's marks"
          subtitle="Printed on report cards and exam permits"
          actions={
            <label className="row" style={{ gap: 8 }}>
              <span className="subtle">Print signatures and stamp</span>
              <Switch
                checked={overview.printSignatures}
                label="Print signatures and stamp"
                onChange={(on) => {
                  api
                    .setPrintSignatures(on)
                    .then(() => {
                      toast("success", on ? "Signatures and stamp will print." : "Documents will print with blank signature lines.");
                      load();
                    })
                    .catch(reportError);
                }}
              />
            </label>
          }
        >
          <div className="sig-grid">
            {slot("head_teacher", null, "Headteacher's signature", overview.headTeacher)}
            {slot("stamp", null, "School stamp", overview.stamp, true)}
          </div>
          {!overview.printSignatures && (
            <Alert tone="warning">Printing is switched off: documents show blank lines to sign by hand.</Alert>
          )}
        </Card>
      )}

      <Card
        title={overview.canManageSchool ? "Staff signatures" : "Your signature"}
        subtitle={
          overview.canManageSchool
            ? "Each Class Teacher's signature prints beside their comment on report cards"
            : "Printed beside your comment on the report cards of your class"
        }
      >
        <div className="sig-grid">
          {overview.staff.map((person) =>
            slot("staff", person.userId, overview.canManageSchool ? person.fullName : "Your signature", person.dataUrl),
          )}
        </div>
      </Card>

      <Modal
        open={pending !== null}
        title={pending ? `Use this as ${pending.title.toLowerCase()}?` : ""}
        description="This is exactly how it will print."
        onClose={() => {
          if (pending) URL.revokeObjectURL(pending.url);
          setPending(null);
        }}
        footer={
          <>
            <Button onClick={() => setPending(null)}>Try another photo</Button>
            <Button variant="primary" loading={busy === "save"} onClick={save}>
              Save
            </Button>
          </>
        }
      >
        {pending && (
          <div className="sig-proof">
            {pending.kind === "stamp" ? (
              <div className="sig-proof-stamp">
                <img src={pending.url} alt="" />
              </div>
            ) : (
              <div className="sig-proof-line">
                <img src={pending.url} alt="" />
                <span>
                  <PenLine size={12} /> Signature
                </span>
              </div>
            )}
            <p className="subtle" style={{ marginTop: 12 }}>
              {pending.kind === "stamp" ? <StampIcon size={12} /> : null} The background is transparent: anything
              that looks like paper here will not print. If part of the ink is missing, retake the photo in brighter
              light.
            </p>
          </div>
        )}
      </Modal>
    </div>
  );
}
