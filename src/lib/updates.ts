/**
 * When to check for updates, and where that decision is remembered.
 *
 * The check itself lives in Rust (`src-tauri/src/updates.rs`). This file owns
 * only the timing, because the timing is the part with a rule worth testing:
 * an update check must never be so eager that it becomes a heartbeat, and
 * never so lazy that a fix sits unnoticed for a week.
 *
 * Three constraints shape it:
 *
 *   1. **Startup is never blocked.** The first check happens after the window
 *      has rendered and the first snapshot has arrived, on a delay. Nothing
 *      about launching LocalDocks waits on GitHub.
 *   2. **At most one check a day.** Opening the app eleven times in an
 *      afternoon is one check, not eleven. The last check time is persisted,
 *      so this holds across restarts rather than only within a session.
 *   3. **The user can always ask.** A manual check ignores all of the above
 *      and runs immediately.
 */

const LAST_CHECK_KEY = 'localdocks.updates.lastCheckedAt.v1';

/** How long after the window renders the first automatic check may run. */
export const STARTUP_DELAY_MS = 8_000;

/** The floor between two automatic checks. Manual checks ignore it. */
export const RECHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/**
 * Should an automatic check run now?
 *
 * `null` means "never checked on this machine", which is the first-run case
 * and is the one time an automatic check happens without a wait.
 *
 * A timestamp in the future is treated as due rather than trusted. Clocks go
 * backwards — a timezone change, an NTP correction, a restored disk image —
 * and a stored value from the future would otherwise suppress every check
 * until real time caught up with it.
 */
export function isCheckDue(lastCheckedAt: number | null, now: number): boolean {
  if (lastCheckedAt === null) return true;
  if (!Number.isFinite(lastCheckedAt)) return true;
  if (lastCheckedAt > now) return true;
  return now - lastCheckedAt >= RECHECK_INTERVAL_MS;
}

/**
 * When the last automatic check ran, or `null`.
 *
 * Guarded like every other storage access in this app: a webview with storage
 * disabled falls back to "never checked" rather than throwing on boot.
 */
export function readLastChecked(): number | null {
  try {
    const raw = window.localStorage.getItem(LAST_CHECK_KEY);
    if (!raw) return null;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function writeLastChecked(now: number): void {
  try {
    window.localStorage.setItem(LAST_CHECK_KEY, String(now));
  } catch {
    // Storage unavailable. The next launch checks again, which is harmless.
  }
}

/** Exported for tests. */
export const __test = { LAST_CHECK_KEY };
