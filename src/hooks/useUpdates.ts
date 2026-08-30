import { useCallback, useEffect, useRef, useState } from 'react';
import type { UpdateCapability, UpdateCheck } from '../types';
import { checkForUpdate, getUpdateCapability, installUpdate } from '../lib/ipc';
import {
  isCheckDue,
  readLastChecked,
  STARTUP_DELAY_MS,
  writeLastChecked,
} from '../lib/updates';

/**
 * The update state machine, in one place.
 *
 * `idle` before anything has been asked, `checking` while a request is in
 * flight, `installing` between the user agreeing and the process being
 * replaced, and otherwise whatever the last check said.
 */
export type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'installing' }
  | { kind: 'installFailed'; reason: string }
  | UpdateCheck;

export interface Updates {
  /** Null until the backend has answered. */
  capability: UpdateCapability | null;
  state: UpdateState;
  /** Runs a check immediately, ignoring the once-a-day floor. */
  check: () => void;
  /** Installs what the last check approved, then restarts. */
  install: () => void;
  /** True when there is a newer stable release the user could install. */
  available: boolean;
}

/**
 * Update checking, wired to the settings toggle.
 *
 * Two rules this hook exists to hold:
 *
 *   * **Nothing here blocks startup.** The first automatic check waits
 *     `STARTUP_DELAY_MS` after mount, by which time the window is up and the
 *     first snapshot has arrived. If the app is closed before then, no request
 *     is ever made.
 *   * **No network without consent.** When `enabled` is false, this hook makes
 *     exactly one IPC call — the local, offline capability query — and never
 *     touches the network. The manual `check` still works, because pressing a
 *     button that says "Check now" is consent.
 *
 * The timer is cleared on unmount, so toggling the setting off mid-countdown
 * cancels the pending check rather than letting it fire.
 */
export function useUpdates(enabled: boolean): Updates {
  const [capability, setCapability] = useState<UpdateCapability | null>(null);
  const [state, setState] = useState<UpdateState>({ kind: 'idle' });

  /* Guards a second check from starting while one is in flight — a double
     click on "Check now", or a manual check racing the startup timer. */
  const inFlight = useRef(false);

  const run = useCallback(async (automatic: boolean) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setState({ kind: 'checking' });
    try {
      const result = await checkForUpdate();
      setState(result);
      /* Recorded for any completed check, including a failed one. A machine
         that is offline all week must not retry every launch. */
      if (automatic || result.kind !== 'failed') writeLastChecked(Date.now());
    } finally {
      inFlight.current = false;
    }
  }, []);

  useEffect(() => {
    let alive = true;
    getUpdateCapability().then((c) => {
      if (alive) setCapability(c);
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!enabled) return;
    if (capability === null || capability.managedByStore) return;
    if (!isCheckDue(readLastChecked(), Date.now())) return;

    const id = window.setTimeout(() => void run(true), STARTUP_DELAY_MS);
    return () => window.clearTimeout(id);
  }, [enabled, capability, run]);

  const check = useCallback(() => void run(false), [run]);

  const install = useCallback(() => {
    setState({ kind: 'installing' });
    /* Resolves only on failure — a successful install replaces this process,
       so there is no success branch to write. */
    installUpdate().catch((err: unknown) => {
      setState({
        kind: 'installFailed',
        reason: typeof err === 'string' ? err : 'The update could not be installed.',
      });
    });
  }, []);

  return {
    capability,
    state,
    check,
    install,
    available: state.kind === 'available',
  };
}
