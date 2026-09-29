/**
 * The last line of defence.
 *
 * A React render error unmounts the entire tree, and what a headteacher sees is
 * a blank window with no explanation and no way forward. That is the worst
 * possible failure for a product whose pitch is "your data is safe here" — it
 * looks exactly like data loss even when nothing has been lost.
 *
 * So: catch it, say plainly what happened, state clearly that nothing was lost,
 * and give one button that gets them working again.
 */

import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertOctagon, Copy, RotateCcw } from "lucide-react";

import { Button } from "./ui";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  componentStack: string | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Goes to the dev console and, in a packaged build, to the webview log.
    console.error("Phantom School Manager hit an unrecoverable error:", error, info);
    this.setState({ componentStack: info.componentStack ?? null });
  }

  render() {
    const { error, componentStack } = this.state;
    if (!error) return this.props.children;

    const detail = [
      error.message,
      error.stack ?? "",
      componentStack ? `\nComponent stack:${componentStack}` : "",
    ]
      .filter(Boolean)
      .join("\n");

    return (
      <div className="auth-panel" style={{ alignItems: "start", paddingTop: "10vh" }}>
        <div className="auth-form" style={{ maxWidth: 640 }}>
          <div className="row" style={{ gap: "var(--space-3)" }}>
            <span
              className="empty-icon"
              style={{
                width: 40,
                height: 40,
                background: "var(--danger-soft)",
                color: "var(--danger)",
              }}
            >
              <AlertOctagon size={20} />
            </span>
            <div>
              <h1 className="auth-title">This screen could not be drawn</h1>
              <p className="auth-note">
                Nothing has been lost. Your school's records are on disk exactly
                as they were a moment ago — this is a fault in the display only.
              </p>
            </div>
          </div>

          <div
            className="selectable"
            style={{
              maxHeight: "40vh",
              overflow: "auto",
              padding: "var(--space-4)",
              background: "var(--bg-sunken)",
              border: "1px solid var(--border-subtle)",
              borderRadius: "var(--radius-md)",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-xs)",
              lineHeight: 1.6,
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {detail}
          </div>

          <div className="row">
            <Button
              variant="primary"
              icon={<RotateCcw size={15} />}
              onClick={() => window.location.reload()}
            >
              Reload Phantom School Manager
            </Button>
            <Button
              icon={<Copy size={15} />}
              onClick={() => {
                void navigator.clipboard.writeText(detail).catch(() => undefined);
              }}
            >
              Copy the details
            </Button>
          </div>

          <p className="field-hint">
            Send the details above to whoever supports your school's copy of
            Phantom School Manager. They say exactly which screen failed and why.
          </p>
        </div>
      </div>
    );
  }
}
