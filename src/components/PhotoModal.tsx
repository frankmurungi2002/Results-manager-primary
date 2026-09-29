/**
 * Choose, preview and save one person's photo — a learner's (FR-C8) or a
 * staff member's (FR-G16). The photo prints on ID cards and report cards.
 */

import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Camera, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import { useStore } from "../state/store";
import { Alert, Button, Loading, Modal } from "./ui";

function toUrl(bytes: number[]): string {
  return URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
}

export function PhotoModal({
  name,
  open,
  load,
  save,
  onClose,
}: {
  name: string;
  open: boolean;
  load: () => Promise<number[] | null>;
  save: (png: number[] | null) => Promise<void>;
  onClose: () => void;
}) {
  const reportError = useStore((state) => state.reportError);
  const toast = useStore((state) => state.toast);

  const [photosOn, setPhotosOn] = useState<boolean | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [picked, setPicked] = useState<number[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setPicked(null);
    setCurrent(null);
    Promise.all([api.getFeatures(), load()])
      .then(([features, bytes]) => {
        setPhotosOn(features.studentPhotos);
        setCurrent(bytes && bytes.length > 0 ? toUrl(bytes) : null);
      })
      .catch(reportError)
      .finally(() => setLoading(false));
    // `load` is a fresh closure each render; the person is what matters.
  }, [open, name, reportError]);

  if (!open) return null;

  async function choose() {
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "PNG image", extensions: ["png"] }],
      });
      if (typeof path !== "string") return;
      setPicked(await api.readImageFile(path));
    } catch (error) {
      reportError(error);
    }
  }

  async function commit(png: number[] | null) {
    setBusy(true);
    try {
      await save(png);
      toast("success", png ? "Photo saved." : "Photo removed.");
      onClose();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  const shown = picked ? toUrl(picked) : current;

  return (
    <Modal
      open
      title={`Photo of ${name}`}
      description="A PNG up to 2 MB. A head-and-shoulders photo, taller than it is wide, prints best."
      onClose={onClose}
      footer={
        <>
          {current && !picked && (
            <Button
              variant="ghost"
              icon={<Trash2 size={15} />}
              loading={busy}
              onClick={() => void commit(null)}
            >
              Remove
            </Button>
          )}
          <span className="grow" />
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!picked}
            onClick={() => void commit(picked)}
          >
            Save photo
          </Button>
        </>
      }
    >
      {loading ? (
        <Loading label="Loading" />
      ) : (
        <div className="stack">
          {photosOn === false && (
            <Alert tone="warning" title="Student Photos is switched off">
              Turn it on under Settings, Optional features. Until then no
              photo prints on an ID card or report card.
            </Alert>
          )}
          <div className="row" style={{ gap: "var(--space-5)", alignItems: "center" }}>
            <div className="photo-frame">
              {shown ? <img src={shown} alt="" /> : <Camera size={26} className="subtle" />}
            </div>
            <div className="stack" style={{ gap: "var(--space-2)" }}>
              <Button icon={<Camera size={15} />} onClick={() => void choose()}>
                {shown ? "Choose a different photo" : "Choose a photo"}
              </Button>
              {picked && <span className="field-hint">Not saved yet.</span>}
            </div>
          </div>
        </div>
      )}
    </Modal>
  );
}
