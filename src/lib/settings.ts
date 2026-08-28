import type { Theme } from '../types';

/**
 * Persisted settings.
 *
 * localStorage, deliberately — it is the simplest mechanism that survives a
 * restart, needs no dependency, and works identically in a browser and in
 * WebView2. A Tauri store plugin would be more idiomatic for a desktop app but
 * buys nothing at this size.
 *
 * Every access is guarded: a webview with storage disabled must fall back to
 * defaults rather than throw on boot.
 */

const KEY = 'localdocks.settings.v1';

export interface Settings {
  theme: Theme;
  intervalMs: number;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  intervalMs: 1000,
};

const THEMES: Theme[] = ['system', 'light', 'dark'];
export const INTERVALS = [500, 1000, 2000, 5000];

/**
 * Validate rather than trust. Stored JSON is user-editable and survives
 * across versions, so a stale or hand-edited value must not reach the UI.
 */
function coerce(raw: unknown): Settings {
  if (typeof raw !== 'object' || raw === null) return DEFAULT_SETTINGS;
  const obj = raw as Record<string, unknown>;

  const theme = THEMES.includes(obj.theme as Theme)
    ? (obj.theme as Theme)
    : DEFAULT_SETTINGS.theme;

  const intervalMs = INTERVALS.includes(obj.intervalMs as number)
    ? (obj.intervalMs as number)
    : DEFAULT_SETTINGS.intervalMs;

  return { theme, intervalMs };
}

export function loadSettings(): Settings {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return DEFAULT_SETTINGS;
    return coerce(JSON.parse(raw));
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(settings: Settings): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    // Storage unavailable or full. Settings stay in memory for this session.
  }
}

/** Exported for tests — the validation is the part worth covering. */
export const __test = { coerce, KEY };
