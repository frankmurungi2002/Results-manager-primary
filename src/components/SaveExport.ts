/**
 * Keeping a file a command made: the system Save dialog picks where, and the
 * backend copies it there. The webview never writes to disk itself.
 */

import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import { api } from "../lib/api";
import type { StagedFile } from "../lib/types";

/** Returns where the file went, or null when the dialog was cancelled. */
export async function keepExport(file: StagedFile): Promise<string | null> {
  if (file.kind === "folder") {
    const folder = await openDialog({ directory: true, title: `Choose where to put "${file.suggestedName}"` });
    if (!folder || Array.isArray(folder)) return null;
    return api.saveExport(file.file, folder);
  }
  const ext = file.kind;
  const destination = await saveDialog({
    defaultPath: file.suggestedName,
    filters: [{ name: ext === "xlsx" ? "Excel workbook" : ext.toUpperCase(), extensions: [ext] }],
  });
  if (!destination) return null;
  return api.saveExport(file.file, destination);
}
