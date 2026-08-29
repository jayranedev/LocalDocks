import type { SystemTelemetry } from '../types';
import { formatBytes } from '../lib/format';
import { SectionLabel } from './ui';

/**
 * Machine-wide load.
 *
 * Kept visually separate from the service tiles above it because it measures a
 * different thing: those numbers are the sum of what LocalDocks can see, this
 * is what the machine is actually doing. Putting a machine CPU figure in the
 * same row as "CPU across services" would invite the two to be read as the
 * same measurement, and they are not — the difference is everything the app
 * cannot open a handle to.
 *
 * Every field is nullable and every null renders as "—". A reading Windows
 * does not expose to an unelevated process is absent from the contract
 * entirely rather than shown as zero, so there is nothing here that can be
 * mistaken for a measurement that was never taken.
 */
export function SystemStrip({ system }: { system: SystemTelemetry }) {
  const { cpuPercent, perCorePercent, memoryTotalBytes, memoryUsedBytes, memoryPercent } = system;

  return (
    <section className="mt-[22px]" aria-label="System load">
      <div className="mb-[9px] flex items-baseline gap-2.5">
        <SectionLabel>SYSTEM</SectionLabel>
        <span className="h-px flex-1 bg-border" />
        <span className="text-[10.5px] text-muted">
          {system.logicalProcessors} logical processors
        </span>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Panel
          label="CPU"
          value={cpuPercent === null ? '—' : `${cpuPercent.toFixed(1)}%`}
          sub={
            cpuPercent === null
              ? 'waiting for a second sample'
              : `across ${system.logicalProcessors} logical processors`
          }
        >
          {perCorePercent === null ? (
            <p className="mt-2 text-[11px] text-muted">Per-core detail unavailable.</p>
          ) : (
            <CoreBars cores={perCorePercent} />
          )}
        </Panel>

        <Panel
          label="MEMORY"
          value={memoryPercent === null ? '—' : `${memoryPercent.toFixed(1)}%`}
          sub={
            memoryUsedBytes === null || memoryTotalBytes === null
              ? 'not measured'
              : `${formatBytes(memoryUsedBytes)} of ${formatBytes(memoryTotalBytes)} in use`
          }
        >
          <Meter percent={memoryPercent} />
          <p className="mt-2 text-[10.5px] leading-[1.5] text-muted">
            Available memory counts the reclaimable cache, which is what Windows itself reports as
            usable.
          </p>
        </Panel>
      </div>
    </section>
  );
}

function Panel({
  label,
  value,
  sub,
  children,
}: {
  label: string;
  value: string;
  sub: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-[9px] border border-border bg-surface px-3.5 py-3">
      <div className="flex items-baseline justify-between">
        <span className="text-[10px] font-semibold tracking-[0.06em] text-muted">{label}</span>
        <span className="font-mono text-[17px] text-primary tabular-nums">{value}</span>
      </div>
      <div className="mt-0.5 text-[11px] text-muted">{sub}</div>
      {children}
    </div>
  );
}

/** One bar per logical processor, in the order Windows enumerates them. */
function CoreBars({ cores }: { cores: number[] }) {
  return (
    <div className="mt-2.5 flex items-end gap-[3px]" role="img" aria-label={coreSummary(cores)}>
      {cores.map((percent, i) => (
        <div
          key={i}
          className="relative h-[26px] flex-1 overflow-hidden rounded-[2px] bg-surface-hover"
          title={`CPU ${i}: ${percent.toFixed(0)}%`}
        >
          <div
            className="absolute inset-x-0 bottom-0 bg-accent transition-[height] duration-300 ease-out motion-reduce:transition-none"
            style={{ height: `${Math.max(2, Math.min(100, percent))}%` }}
          />
        </div>
      ))}
    </div>
  );
}

function coreSummary(cores: number[]): string {
  const busiest = cores.reduce((a, b) => Math.max(a, b), 0);
  return `${cores.length} logical processors, busiest at ${busiest.toFixed(0)} percent`;
}

function Meter({ percent }: { percent: number | null }) {
  return (
    <div className="mt-2.5 h-[6px] overflow-hidden rounded-full bg-surface-hover">
      {percent !== null && (
        <div
          className="h-full bg-accent transition-[width] duration-300 ease-out motion-reduce:transition-none"
          style={{ width: `${Math.max(0, Math.min(100, percent))}%` }}
        />
      )}
    </div>
  );
}
