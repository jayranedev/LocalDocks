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

/** Applies the theme choice to the document, following the OS when set to system. */
export function useTheme(theme: Theme): void {
  useEffect(() => {
    const root = document.documentElement;

    if (theme !== 'system') {
      root.setAttribute('data-theme', theme);
      return;
    }

    const media = window.matchMedia('(prefers-color-scheme: light)');
    const apply = () => root.setAttribute('data-theme', media.matches ? 'light' : 'dark');
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [theme]);
}
