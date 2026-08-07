// Thin, deferred access to the Tauri window so the plain-browser preview
// build can stay free of the native module graph.

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./api";

interface WindowApi {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  setOpacity(opacity: number): Promise<void>;
}

let cached: WindowApi | null = null;

function call(op: "minimize" | "toggleMaximize" | "close"): Promise<void> {
  return import("@tauri-apps/api/window")
    .then((m) => {
      const w = m.getCurrentWindow();
      if (op === "minimize") return w.minimize();
      if (op === "toggleMaximize") return w.toggleMaximize();
      return w.close();
    })
    .catch((err: unknown) => {
      console.error(`[windowApi] ${op} failed`, err);
      throw err;
    });
}

function applyOpacity(opacity: number): Promise<void> {
  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty("--app-opacity", opacity.toString());
  }
  if (!isTauri) return Promise.resolve();
  return invoke<void>("set_window_opacity", { opacity }).catch((err: unknown) => {
    console.error("[windowApi] set_window_opacity failed", err);
    throw err;
  });
}

function api(): WindowApi | null {
  if (!isTauri) return null;
  if (!cached) {
    cached = {
      minimize: () => call("minimize"),
      toggleMaximize: () => call("toggleMaximize"),
      close: () => call("close"),
      setOpacity: (opacity: number) => applyOpacity(opacity),
    };
  }
  return cached;
}

export function win(): WindowApi | null {
  return api();
}
