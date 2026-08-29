import { useEffect, useRef, useState } from 'react';
import type { LoadState, Snapshot, Theme } from '../types';
import { DEFAULT_INTERVAL_MS, subscribeSnapshot } from '../lib/ipc';

/**
 * Subscribes to sampler ticks and derives the screen's load state.
 *
 * Deliberately thin. It does not poll, it does not own a timer, and it does
 * not decide when a scan happens — it receives snapshots. If this hook ever
 * grows a `setInterval` that calls into `ipc`, the architecture has regressed.
 *
 * On failure it keeps the last good snapshot so a single bad tick degrades
 * into a warning rather than blanking a working UI.
 */
export function useSnapshot(intervalMs: number = DEFAULT_INTERVAL_MS): LoadState {
  const [state, setState] = useState<LoadState>({ kind: 'loading' });
  const lastGood = useRef<Snapshot | null>(null);

  useEffect(() => {
    let cancelled = false;

    const stop = subscribeSnapshot({
      intervalMs,
      onTick: (snapshot) => {
        if (cancelled) return;
        lastGood.current = snapshot;
        setState(
          snapshot.services.length === 0
            ? { kind: 'empty', snapshot }
            : { kind: 'ready', snapshot },
        );
      },
      onError: (message, detail) => {
        if (cancelled) return;
        setState({ kind: 'error', message, detail, stale: lastGood.current });
      },
    });

    return () => {
      cancelled = true;
      stop();
    };
  }, [intervalMs]);

  return state;
}

/** Re-renders on a timer so "scanned N s ago" actually counts up. */
export function useTicker(ms = 100): number {
  const [n, setN] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setN((v) => v + 1), ms);
    return () => window.clearInterval(id);
  }, [ms]);
  return n;
}

/**
 * Applies the chosen theme to the document.
 *
 * One attribute write, no OS listener: the three themes are explicit choices,
 * so nothing outside the app is allowed to change how it looks. Every colour
 * downstream resolves from the `[data-theme]` block this selects, which is why
 * no component needs to know which theme is active.
 */
export function useTheme(theme: Theme): void {
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);
}
