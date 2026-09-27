/**
 * A development-only strip that says which branch of the application is on
 * screen right now.
 *
 * It exists because "the window is blank" is the least informative bug report
 * there is: it looks identical whether React crashed, a screen returned
 * nothing, or the session was silently dropped. This renders *outside* `App`,
 * so it survives anything `App` does — including returning null.
 *
 * Stripped from production builds by `import.meta.env.DEV`.
 */

import { useStore } from "../state/store";

export function BootDiagnostics() {
  const booted = useStore((state) => state.booted);
  const bootError = useStore((state) => state.bootError);
  const provisioned = useStore((state) => state.provisioned);
  const session = useStore((state) => state.session);
  const institution = useStore((state) => state.institution);
  const screen = useStore((state) => state.screen);
  const theme = useStore((state) => state.theme);

  const branch = !booted
    ? "loading"
    : bootError
      ? "boot error"
      : !provisioned
        ? "setup wizard"
        : !session
          ? "login"
          : `shell → ${screen}`;

  return (
    <div
      style={{
        position: "fixed",
        left: 8,
        bottom: 8,
        zIndex: 9999,
        padding: "6px 10px",
        borderRadius: 6,
        background: "rgba(12, 14, 24, 0.92)",
        border: "1px solid rgba(255,255,255,0.16)",
        color: "#cfd4e6",
        font: "11px/1.5 ui-monospace, Consolas, monospace",
        pointerEvents: "none",
        whiteSpace: "pre",
      }}
    >
      {[
        `branch    ${branch}`,
        `booted    ${booted}`,
        `provision ${provisioned}`,
        `session   ${session ? `${session.username} (${session.role})` : "none"}`,
        `school    ${institution?.name ?? "not loaded"}`,
        `theme     ${theme}`,
        bootError ? `error     ${bootError}` : null,
      ]
        .filter(Boolean)
        .join("\n")}
    </div>
  );
}
