import type { AppMode, Theme } from '../types';

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
  mode: AppMode;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: 'local-dark',
  intervalMs: 1000,
  /* Developer is the default: the app is for watching your own services, and
     the full machine view is one switch away. */
  mode: 'developer',
};

export const MODES: AppMode[] = ['system', 'developer'];

export const THEMES: Theme[] = ['local-dark', 'dark', 'light'];
export const INTERVALS = [500, 1000, 2000, 5000];

/** What the theme toggle shows. Kept beside the list it labels. */
export const THEME_LABELS: Record<Theme, string> = {
  'local-dark': 'Local Dark',
  dark: 'Dark',
  light: 'Light',
};

/**
 * Validate rather than trust. Stored JSON is user-editable and survives
 * across versions, so a stale or hand-edited value must not reach the UI.
 */
function coerce(raw: unknown): Settings {
  if (typeof raw !== 'object' || raw === null) return DEFAULT_SETTINGS;
  const obj = raw as Record<string, unknown>;

  // A stored 'system' from before explicit theming, or anything hand-edited,
  // falls through to the default rather than reaching the UI as a data-theme
  // value with no matching CSS block.
  const theme = THEMES.includes(obj.theme as Theme)
    ? (obj.theme as Theme)
    : DEFAULT_SETTINGS.theme;

  const intervalMs = INTERVALS.includes(obj.intervalMs as number)
    ? (obj.intervalMs as number)
    : DEFAULT_SETTINGS.intervalMs;

  // Absent in settings written before the mode existed, which is the common
  // case on first run after an update — it falls through to the default.
  const mode = MODES.includes(obj.mode as AppMode)
    ? (obj.mode as AppMode)
    : DEFAULT_SETTINGS.mode;

  return { theme, intervalMs, mode };
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
