// Loaded before the interface in a plain browser (see
// src-tauri/examples/ui_bridge.rs). Stands in for Tauri's IPC: every command
// goes to the real backend over a local port, so what the page shows is what
// the real commands return. Development and screenshots only.
(function () {
  const port = (window.__BRIDGE_PORT || 4399);
  const url = `http://127.0.0.1:${port}/invoke`;
  window.__calls = [];
  // Commands that came back with an error, for checks to read.
  window.__fails = [];
  // A file the next "open file" dialog returns, set by the test.
  window.__nextFile = null;
  // Where the next "save file" dialog says to write, set by the test.
  window.__nextSave = null;
  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => {
      const id = Math.floor(Math.random() * 1e9);
      window[`_${id}`] = cb;
      return id;
    },
    invoke: async (cmd, args) => {
      window.__calls.push(cmd);
      if (cmd === "plugin:dialog|open") return window.__nextFile;
      if (cmd === "plugin:dialog|save") return window.__nextSave;
      if (cmd.startsWith("plugin:")) return null;
      const response = await fetch(url, { method: "POST", body: JSON.stringify({ cmd, args: args ?? {} }) });
      const out = await response.json();
      if ("err" in out) {
        window.__fails.push(`${cmd}: ${out.err && out.err.code}`);
        throw out.err;
      }
      return out.ok;
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
  };
})();
