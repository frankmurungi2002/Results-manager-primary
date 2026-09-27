import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and fails the dev command if it is taken.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],

  // Prevent Vite from obscuring Rust errors.
  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // The Rust crate rebuilds itself; Vite has no business watching it.
      ignored: ["**/src-tauri/**"],
    },
  },

  // Target the webview engines Tauri actually ships against.
  build: {
    target: "chrome105",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
  },

  envPrefix: ["VITE_", "TAURI_ENV_"],
});
