import type { AppMode } from '../types';
import { BACKEND } from '../lib/ipc';
import { MODE_LABELS } from '../lib/view';
import { Icon } from './Icon';

interface StatusBarProps {
  /** The active presentation mode, so the current view is never ambiguous. */
  mode: AppMode;
  /** What the mode is holding back, counted from the same view model. */
  hidden: { processes: number; ports: number };
  serviceCount: number;
  /** null while the backend does not compute conflicts — renders "—", not "0". */
  conflicts: number | null;
  /** Seconds since the last sampler tick. */
  age: number;
  intervalMs: number;
  /** Present while scans are failing; the data shown is the last good snapshot. */
  error: { message: string; detail: string } | null;
  /**
   * Set when a newer stable release exists. Rendered as one quiet, clickable
   * word — an update is worth mentioning, never worth interrupting a scan for.
   */
  updateAvailable: { version: string; onOpen: () => void } | null;
}

export function StatusBar({
  mode,
  hidden,
  serviceCount,
  conflicts,
  age,
  intervalMs,
  error,
  updateAvailable,
}: StatusBarProps) {
  return (
    <footer className="flex h-[30px] flex-none items-center gap-3.5 border-t border-border bg-surface-raised px-3.5 text-[11.5px] text-muted">
      <span className="flex items-center gap-[7px]">
        <span className={`size-1.5 rounded-full ${error ? 'bg-danger' : 'bg-success'}`} />
        <span className="text-secondary">
          {serviceCount} {serviceCount === 1 ? 'service' : 'services'}
        </span>
      </span>

      <span
        className={conflicts !== null && conflicts > 0 ? 'text-warning' : undefined}
        title={conflicts === null ? 'Conflict detection is not implemented yet' : undefined}
      >
        {conflicts === null ? '— conflicts' : `${conflicts} conflicts`}
      </span>

      <span
        className={mode === 'developer' ? 'text-accent' : undefined}
        title={
          mode === 'developer'
            ? `Developer mode — ${hidden.processes} processes and ${hidden.ports} sockets hidden`
            : 'System mode — nothing hidden'
        }
      >
        {MODE_LABELS[mode]}
      </span>

      <div className="flex-1" />

      {updateAvailable && (
        <>
          <button
            type="button"
            onClick={updateAvailable.onOpen}
            title={`LocalDocks ${updateAvailable.version} is available — open Settings to install it`}
            className="flex items-center gap-[5px] rounded-[5px] px-1 text-accent transition-colors hover:underline"
          >
            <Icon name="download" size={13} />
            <span>v{updateAvailable.version} available</span>
          </button>
          <span>·</span>
        </>
      )}

      {error ? (
        <span className="flex items-center gap-[5px] text-danger" title={error.detail}>
          <Icon name="warn" size={13} />
          <span>scan failing — showing last good snapshot</span>
        </span>
      ) : (
        <>
          {BACKEND === 'mock' && (
            <>
              <span className="text-warning">mock backend</span>
              <span>·</span>
            </>
          )}
          <span className="font-mono">sampler {intervalMs} ms</span>
          <span>·</span>
          <span className="tabular-nums">scanned {age.toFixed(1)} s ago</span>
        </>
      )}
      <span>·</span>
      <span className="flex items-center gap-[5px]">
        <Icon name="lock" size={13} />
        unelevated
      </span>
    </footer>
  );
}
