import type {
  ProcessDetail,
  ProcessId,
  Snapshot,
  TerminateRequest,
  TerminateResult,
  UpdateCapability,
  UpdateCheck,
} from '../types';
import { buildDetail, buildSnapshot, mockTerminate } from './mock';

/**
 * The IPC seam.
 *
 * Every crossing between React and Rust goes through this file and nowhere
 * else. No component imports `@tauri-apps/api` directly.
 *
 * Two consequences worth the discipline:
 *
 *  1. The whole UI runs in a plain browser via `npm run dev`, against the mock,
 *     with no Rust toolchain involved.
 *  2. When the sampler lands, only this file changes. Every screen, hook and
 *     component keeps working untouched.
 *
 * Note the shape of `subscribeSnapshot`: the frontend does not poll. It asks to
 * be told. That mirrors the real architecture, where the Rust sampler owns the
 * cadence and pushes `services:update` — a React render must never be able to
 * trigger a Windows syscall.
 */

/** Tauri injects this. Absent in a plain browser, so we fall back to the mock. */
const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const BACKEND: 'tauri' | 'mock' = IS_TAURI ? 'tauri' : 'mock';

export const DEFAULT_INTERVAL_MS = 1000;

/** Normalises anything thrown across the IPC boundary into a displayable pair. */
function describeError(err: unknown): { message: string; detail: string } {
  if (typeof err === 'string') return { message: 'Scan failed', detail: err };
  if (err instanceof Error) return { message: 'Scan failed', detail: err.message };
  try {
    return { message: 'Scan failed', detail: JSON.stringify(err) };
  } catch {
    return { message: 'Scan failed', detail: 'Unknown error' };
  }
}

/**
 * The application version, read from the running app rather than written here.
 *
 * There is exactly one version number in this repository —
 * `src-tauri/Cargo.toml` — and this is how the UI reaches it. Hard-coding it in
 * a component is how the About screen came to claim `v0.1.0` while the
 * installer, the executable metadata and the uninstall entry all said `0.9.0`.
 * A version the user can read has to come from the same place as the version
 * the installer writes, or the two will disagree exactly when it matters.
 *
 * `null` in a browser-only dev session, where there is no packaged app to ask.
 */
export async function getAppVersion(): Promise<string | null> {
  if (!IS_TAURI) return null;
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch {
    // A version we cannot read is shown as absent rather than guessed.
    return null;
  }
}

export async function getSnapshot(): Promise<Snapshot> {
  if (!IS_TAURI) return buildSnapshot();
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<Snapshot>('get_snapshot');
}

export interface SnapshotSubscription {
  onTick: (snapshot: Snapshot) => void;
  /** Called when a scan fails. Never synthesised — only real failures. */
  onError: (message: string, detail: string) => void;
  intervalMs?: number;
}

/**
 * Subscribe to sampler ticks. Returns an unsubscribe function.
 *
 * Real implementation listens for `services:update`, and for `services:error`
 * which the sampler emits when a scan throws. The mock runs a timer, which is
 * the closest a browser can get to "something else owns the cadence".
 *
 * The mock has no failure injection on purpose: `onError` exists because the
 * real backend can fail, not so the error UI has something to render.
 */
export function subscribeSnapshot({
  onTick,
  onError,
  intervalMs = DEFAULT_INTERVAL_MS,
}: SnapshotSubscription): () => void {
  if (!IS_TAURI) {
    const tick = () => {
      try {
        onTick(buildSnapshot());
      } catch (err) {
        const { message, detail } = describeError(err);
        onError(message, detail);
      }
    };
    tick();
    const id = window.setInterval(tick, intervalMs);
    return () => window.clearInterval(id);
  }

  let disposed = false;
  const stops: Array<() => void> = [];

  void (async () => {
    try {
      const [{ invoke }, { listen }] = await Promise.all([
        import('@tauri-apps/api/core'),
        import('@tauri-apps/api/event'),
      ]);

      // The backend owns the cadence; this only tells it which cadence to own.
      await invoke('set_sample_interval', { intervalMs });

      stops.push(
        await listen<Snapshot>('services:update', (event) => onTick(event.payload)),
        await listen<string>('services:error', (event) => {
          const { message, detail } = describeError(event.payload);
          onError(message, detail);
        }),
      );

      // Seed immediately rather than waiting a full interval for the first tick.
      onTick(await getSnapshot());

      if (disposed) stops.forEach((stop) => stop());
    } catch (err) {
      const { message, detail } = describeError(err);
      if (!disposed) onError(message, detail);
    }
  })();

  return () => {
    disposed = true;
    stops.forEach((stop) => stop());
  };
}

/** Tier-2 data. Called when a detail panel opens — never in the scan loop. */
export async function getProcessDetail(processId: ProcessId): Promise<ProcessDetail> {
  if (!IS_TAURI) {
    // Small delay so the "fetched on open" behaviour is visible in the UI.
    await new Promise((r) => setTimeout(r, 180));
    return buildDetail(processId);
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ProcessDetail>('get_process_detail', { processId });
}

/**
 * Force-terminate a process.
 *
 * `startedAt` is not decoration. The backend re-opens the PID, reads its
 * creation time and refuses when it differs, so a recycled PID cannot be
 * killed by a row the user is looking at from three seconds ago.
 */
export async function terminateProcess(req: TerminateRequest): Promise<TerminateResult> {
  if (!IS_TAURI) {
    await new Promise((r) => setTimeout(r, 220));
    return mockTerminate(req);
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<TerminateResult>('terminate_process', {
    pid: req.pid,
    startedAt: req.startedAt,
  });
}

/** Opens a URL in the user's default browser via the OS, not a webview nav. */
export async function openExternal(url: string): Promise<void> {
  if (!IS_TAURI) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('open_external', { url });
}

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

/**
 * What this installation can do about updates.
 *
 * The backend answers from process state — whether it is running with MSIX
 * package identity — so the UI never has to guess which channel it came from.
 * A browser dev session reports `managedByStore`, because there is no
 * installation to update and pretending otherwise would put a live-looking
 * button in front of nothing.
 */
export async function getUpdateCapability(): Promise<UpdateCapability> {
  if (!IS_TAURI) {
    return { managedByStore: true, currentVersion: '0.0.0-dev' };
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<UpdateCapability>('update_capability');
}

/**
 * Ask GitHub whether a newer stable release exists.
 *
 * The command is infallible by construction — every network and parsing
 * failure arrives as a `failed` variant — but this still catches, because an
 * IPC layer that cannot be reached at all is a different failure from one that
 * answered honestly, and neither should break a running app.
 */
export async function checkForUpdate(): Promise<UpdateCheck> {
  if (!IS_TAURI) {
    return { kind: 'unsupported', reason: 'Updates are unavailable in a browser session.' };
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<UpdateCheck>('check_for_update');
  } catch {
    return { kind: 'failed', reason: 'Could not reach GitHub to check for updates.' };
  }
}

/**
 * Install the update the last check approved and restart into it.
 *
 * Resolves only on failure: a successful install replaces this process, so
 * nothing here runs afterwards. The string it rejects with is displayable.
 */
export async function installUpdate(): Promise<void> {
  if (!IS_TAURI) throw new Error('Updates are unavailable in a browser session.');
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('install_update');
}
