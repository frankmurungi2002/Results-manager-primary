/** Application state: who is signed in, where they are, and what to show them. */

import { create } from "zustand";

import { ApiError, api } from "../lib/api";
import type { FeatureFlags, Institution, SessionView, TeacherMode } from "../lib/types";

export type ScreenId =
  | "dashboard"
  | "marks"
  | "classes"
  | "learners"
  | "reports"
  | "staff"
  | "calendar"
  | "grading"
  | "audit"
  | "settings";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error" | "warning";
  message: string;
}

interface AppStore {
  // --- Boot ---------------------------------------------------------------
  booted: boolean;
  provisioned: boolean;
  appVersion: string;
  bootError: string | null;

  // --- Session ------------------------------------------------------------
  session: SessionView | null;
  institution: Institution | null;
  features: FeatureFlags;

  // --- Navigation ---------------------------------------------------------
  screen: ScreenId;
  screenParams: Record<string, string | undefined>;

  // --- Chrome -------------------------------------------------------------
  theme: "light" | "dark" | "system";
  toasts: Toast[];

  boot: () => Promise<void>;
  signIn: (username: string, password: string) => Promise<void>;
  signOut: () => Promise<void>;
  refreshSession: () => Promise<void>;
  refreshInstitution: () => Promise<void>;
  setTeacherMode: (mode: TeacherMode) => Promise<void>;
  markProvisioned: () => void;

  navigate: (screen: ScreenId, params?: Record<string, string | undefined>) => void;

  setTheme: (theme: "light" | "dark" | "system") => Promise<void>;
  toast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
  reportError: (error: unknown, fallback?: string) => void;
}

const NO_FEATURES: FeatureFlags = {
  studentPhotos: false,
  examPermits: false,
  streams: false,
  weeklyAssignments: false,
};

let toastSequence = 0;

export const useStore = create<AppStore>((set, get) => ({
  booted: false,
  provisioned: false,
  appVersion: "",
  bootError: null,

  session: null,
  institution: null,
  features: NO_FEATURES,

  screen: "dashboard",
  screenParams: {},

  theme: "system",
  toasts: [],

  async boot() {
    try {
      const startup = await api.startupState();
      set({
        booted: true,
        provisioned: startup.provisioned,
        appVersion: startup.appVersion,
        session: startup.session,
        bootError: null,
      });

      if (startup.provisioned) {
        // The theme is a device preference and is readable before sign-in, so
        // the login screen already looks the way the school set it.
        try {
          const theme = (await api.getTheme()) as "light" | "dark" | "system";
          applyTheme(theme);
          set({ theme });
        } catch {
          applyTheme("system");
        }
      }

      if (startup.session) {
        await get().refreshInstitution();
      }
    } catch (error) {
      set({
        booted: true,
        bootError:
          error instanceof ApiError
            ? error.message
            : "Results Manager could not start.",
      });
    }
  },

  async signIn(username, password) {
    const session = await api.signIn(username, password);
    set({ session });
    await get().refreshInstitution();
    // A teacher who is only a Subject Teacher has no business on the
    // Class Teacher screens, so land everyone where their role is useful.
    set({ screen: "dashboard", screenParams: {} });
  },

  async signOut() {
    try {
      await api.signOut();
    } finally {
      set({
        session: null,
        institution: null,
        features: NO_FEATURES,
        screen: "dashboard",
        screenParams: {},
      });
    }
  },

  async refreshSession() {
    try {
      const session = await api.currentSession();
      if (!session && get().session) {
        // The backend timed the session out while the window sat idle.
        set({
          session: null,
          screen: "dashboard",
          screenParams: {},
          toasts: [
            ...get().toasts,
            {
              id: ++toastSequence,
              kind: "info",
              message: "You were signed out after 30 minutes of inactivity.",
            },
          ],
        });
        return;
      }
      set({ session });
    } catch {
      set({ session: null });
    }
  },

  async refreshInstitution() {
    try {
      const [institution, features] = await Promise.all([
        api.getInstitution(),
        api.getFeatures(),
      ]);
      set({ institution, features });
      applyAccent(institution.accentColor);
    } catch {
      // A teacher signing in before setup finishes is not an error worth
      // surfacing; the dashboard will simply show less.
    }
  },

  async setTeacherMode(mode) {
    const session = await api.setTeacherMode(mode);
    set({ session });
  },

  markProvisioned() {
    set({ provisioned: true });
  },

  navigate(screen, params = {}) {
    set({ screen, screenParams: params });
  },

  async setTheme(theme) {
    applyTheme(theme);
    set({ theme });
    try {
      await api.setTheme(theme);
    } catch {
      // A theme that fails to persist is still applied for this session.
    }
  },

  toast(kind, message) {
    const id = ++toastSequence;
    set({ toasts: [...get().toasts, { id, kind, message }] });
    const lifetime = kind === "error" ? 7000 : 3800;
    window.setTimeout(() => get().dismissToast(id), lifetime);
  },

  dismissToast(id) {
    set({ toasts: get().toasts.filter((toast) => toast.id !== id) });
  },

  reportError(error, fallback = "Something went wrong.") {
    if (error instanceof ApiError) {
      if (error.code === "unauthenticated") {
        set({ session: null });
        get().toast("info", "Please sign in again.");
        return;
      }
      get().toast("error", error.message);
      return;
    }
    get().toast("error", fallback);
  },
}));

export function applyTheme(theme: "light" | "dark" | "system") {
  document.documentElement.setAttribute("data-theme", theme);
}

/**
 * FR-B2's per-school accent colour. Applied as a token override so every
 * component picks it up without knowing the school exists.
 */
export function applyAccent(hex: string) {
  const root = document.documentElement;
  if (!/^#[0-9a-fA-F]{6}$/.test(hex)) return;

  root.style.setProperty("--accent", hex);
  root.style.setProperty("--accent-hover", shade(hex, -0.12));
  root.style.setProperty("--accent-active", shade(hex, -0.24));
  root.style.setProperty("--accent-soft", withAlpha(hex, 0.11));
  root.style.setProperty("--accent-soft-border", withAlpha(hex, 0.3));
  root.style.setProperty("--bg-selected", withAlpha(hex, 0.1));
  root.style.setProperty("--border-focus", hex);
  root.style.setProperty("--shadow-focus", `0 0 0 3px ${withAlpha(hex, 0.24)}`);
  root.style.setProperty(
    "--text-on-accent",
    relativeLuminance(hex) > 0.55 ? "#12152a" : "#ffffff",
  );
}

function parse(hex: string): [number, number, number] {
  return [
    parseInt(hex.slice(1, 3), 16),
    parseInt(hex.slice(3, 5), 16),
    parseInt(hex.slice(5, 7), 16),
  ];
}

function shade(hex: string, amount: number): string {
  const channels = parse(hex).map((value) => {
    const shifted = amount < 0 ? value * (1 + amount) : value + (255 - value) * amount;
    return Math.round(Math.min(255, Math.max(0, shifted)));
  });
  return `#${channels.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}

function withAlpha(hex: string, alpha: number): string {
  const [r, g, b] = parse(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** WCAG relative luminance, used to pick readable text on the accent. */
function relativeLuminance(hex: string): number {
  const [r, g, b] = parse(hex).map((value) => {
    const channel = value / 255;
    return channel <= 0.03928
      ? channel / 12.92
      : Math.pow((channel + 0.055) / 1.055, 2.4);
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
