import { BACKEND } from '../lib/ipc';
import { Icon } from './Icon';

interface StatusBarProps {
  serviceCount: number;
  /** null while the backend does not compute conflicts — renders "—", not "0". */
  conflicts: number | null;
  /** Seconds since the last sampler tick. */
  age: number;
  intervalMs: number;
  /** Present while scans are failing; the data shown is the last good snapshot. */
  error: { message: string; detail: string } | null;
}

export function StatusBar({ serviceCount, conflicts, age, intervalMs, error }: StatusBarProps) {
  return (
    <footer className="flex h-[30px] flex-none items-center gap-3.5 border-t border-bd bg-elev px-3.5 text-[11.5px] text-t3">
      <span className="flex items-center gap-[7px]">
        <span className={`size-1.5 rounded-full ${error ? 'bg-red' : 'bg-grn'}`} />
        <span className="text-t2">
          {serviceCount} {serviceCount === 1 ? 'service' : 'services'}
        </span>
      </span>

      <span
        className={conflicts !== null && conflicts > 0 ? 'text-amb' : undefined}
        title={conflicts === null ? 'Conflict detection is not implemented yet' : undefined}
      >
        {conflicts === null ? '— conflicts' : `${conflicts} conflicts`}
      </span>

      <div className="flex-1" />

      {error ? (
        <span className="flex items-center gap-[5px] text-red" title={error.detail}>
          <Icon name="warn" size={13} />
          <span>scan failing — showing last good snapshot</span>
        </span>
      ) : (
        <>
          {BACKEND === 'mock' && (
            <>
              <span className="text-amb">mock backend</span>
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
