import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import "./styles/index.css";

// A desktop application, not a web page: the browser context menu belongs to a
// browser, not to RM. In development it stays, because right-click → Inspect is
// how anyone working on RM reaches the console.
if (import.meta.env.PROD) {
  document.addEventListener("contextmenu", (event) => {
    const target = event.target as HTMLElement;
    const editable =
      target.closest("input, textarea, [contenteditable], .selectable") !== null;
    if (!editable) event.preventDefault();
  });
}

// A promise nobody awaited that rejects leaves no trace on screen and no entry
// in any log a school could send us. Surface both kinds of silent failure.
window.addEventListener("unhandledrejection", (event) => {
  console.error("Unhandled promise rejection:", event.reason);
});
window.addEventListener("error", (event) => {
  console.error("Uncaught error:", event.error ?? event.message);
});

const container = document.getElementById("root");
if (!container) throw new Error("Root element is missing from index.html");

createRoot(container).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
