import type { ReactNode } from 'react';
import type { NetworkTelemetry, SystemTelemetry } from '../types';
import { formatBytes, formatRate } from '../lib/format';
import { SectionLabel } from './ui';

/**
 * Machine-wide telemetry, one card per metric.
 *
 * Kept visually separate from the service tiles above because it measures a
 * different thing. Those numbers are the sum of what LocalDocks can see; these
 * are what the machine is actually doing, and the difference is everything the
 * app cannot open a handle to.
 *
 * # The rule every card follows
 *
 * A metric that was not measured says so in words. It never shows `0%`,
 * `0 MB`, `0 MB/s` or `0 °C`, because a reader cannot tell a failed provider
 * from an idle machine, and the failure is the more likely explanation for a
 * flat zero. The backend already encodes this — null means "not measured",
 * never "measured as zero" — so every card here is a null check rather than a
 * judgement call.
 *
 * Each unavailable state also says *why*, because "Unavailable" alone reads as
 * a bug in the app. "No GPU performance counters on this machine" is a fact the
 * reader can act on.
 *
 * Every colour is a semantic token, so all three themes follow without this
 * file knowing they exist.
 */
export function TelemetryCards({ system }: { system: SystemTelemetry }) {
  return (
    <section className="mt-[22px]" aria-label="System telemetry">
      <div className="mb-[9px] flex items-baseline gap-2.5">
        <SectionLabel>SYSTEM</SectionLabel>
        <span className="h-px flex-1 bg-border" />
        <span className="text-[10.5px] text-muted">machine-wide, not per service</span>
      </div>

      <div className="grid grid-cols-3 gap-3">
        <CpuCard system={system} />
        <MemoryCard system={system} />
        <NetworkCard network={system.network} />
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ shell */

function Card({
  label,
  value,
  sub,
  children,
}: {
  label: string;
  value: string;
  sub: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col rounded-[9px] border border-border bg-surface px-3.5 py-3">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-[10px] font-semibold tracking-[0.06em] text-muted">{label}</span>
        <span className="font-mono text-[17px] whitespace-nowrap text-primary tabular-nums">
          {value}
        </span>
      </div>
      <div className="mt-0.5 text-[11px] text-muted">{sub}</div>
      {children}
    </div>
  );
}

/**
 * A metric this machine does not provide.
 *
 * The em dash sits where a number would, so the card keeps its place in the
 * grid — and `reason` is required rather than optional, because an unexplained
 * "Unavailable" reads as a bug.
 */
function Unavailable({ label, reason }: { label: string; reason: string }) {
  return (
    <div className="flex flex-col rounded-[9px] border border-dashed border-border bg-surface px-3.5 py-3">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-[10px] font-semibold tracking-[0.06em] text-muted">{label}</span>
        <span className="font-mono text-[17px] text-muted">—</span>
      </div>
      <div className="mt-0.5 text-[11px] leading-[1.45] text-muted">{reason}</div>
    </div>
  );
}

/** A filled bar, or an empty track when there is nothing to show. */
function Meter({ percent }: { percent: number | null }) {
  return (
    <div className="mt-2.5 h-[6px] overflow-hidden rounded-full bg-surface-hover">
      {percent !== null && (
        <div
          className="h-full bg-accent transition-[width] duration-300 ease-out motion-reduce:transition-none"
          style={{ width: `${clamp(percent)}%` }}
        />
      )}
    </div>
  );
}

const clamp = (n: number) => Math.max(0, Math.min(100, n));
const pct = (n: number) => `${n.toFixed(1)}%`;

/* ------------------------------------------------------------------- cards */

function CpuCard({ system }: { system: SystemTelemetry }) {
  const { cpuPercent, perCorePercent, logicalProcessors } = system;

  return (
    <Card
      label="CPU"
      value={cpuPercent === null ? '—' : pct(cpuPercent)}
      sub={
        cpuPercent === null
          ? 'waiting for a second sample'
          : `across ${logicalProcessors} logical processors`
      }
    >
      {perCorePercent === null ? (
        <p className="mt-2 text-[10.5px] text-muted">Per-core detail unavailable.</p>
      ) : (
        <div
          className="mt-2.5 flex items-end gap-[3px]"
          role="img"
          aria-label={`${perCorePercent.length} logical processors, busiest at ${Math.max(
            ...perCorePercent,
          ).toFixed(0)} percent`}
        >
          {perCorePercent.map((percent, i) => (
            <div
              key={i}
              className="relative h-[26px] flex-1 overflow-hidden rounded-[2px] bg-surface-hover"
              title={`CPU ${i}: ${percent.toFixed(0)}%`}
            >
              <div
                className="absolute inset-x-0 bottom-0 bg-accent transition-[height] duration-300 ease-out motion-reduce:transition-none"
                style={{ height: `${Math.max(2, clamp(percent))}%` }}
              />
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

function MemoryCard({ system }: { system: SystemTelemetry }) {
  const { memoryPercent, memoryUsedBytes, memoryTotalBytes } = system;
  const measured = memoryUsedBytes !== null && memoryTotalBytes !== null;

  return (
    <Card
      label="MEMORY"
      value={memoryPercent === null ? '—' : pct(memoryPercent)}
      sub={
        measured
          ? `${formatBytes(memoryUsedBytes)} of ${formatBytes(memoryTotalBytes)} in use`
          : 'not measured'
      }
    >
      <Meter percent={memoryPercent} />
      <p className="mt-2 text-[10.5px] leading-[1.45] text-muted">
        Physical memory, not the per-service total above. Available counts the reclaimable cache,
        which is what Windows itself reports as usable.
      </p>
    </Card>
  );
}

function NetworkCard({ network }: { network: NetworkTelemetry | null }) {
  if (network === null) {
    return <Unavailable label="NETWORK" reason="The interface table could not be read." />;
  }

  const { receiveBytesPerSec, transmitBytesPerSec, interfaces } = network;
  const measured = receiveBytesPerSec !== null || transmitBytesPerSec !== null;
  const busiest = [...interfaces]
    .filter((i) => i.receiveBytesPerSec !== null || i.transmitBytesPerSec !== null)
    .sort(
      (a, b) =>
        (b.receiveBytesPerSec ?? 0) +
        (b.transmitBytesPerSec ?? 0) -
        ((a.receiveBytesPerSec ?? 0) + (a.transmitBytesPerSec ?? 0)),
    )[0];

  return (
    <Card
      label="NETWORK"
      value={measured ? formatRate((receiveBytesPerSec ?? 0) + (transmitBytesPerSec ?? 0)) : '—'}
      sub={
        measured
          ? `${interfaces.length} active interface${interfaces.length === 1 ? '' : 's'}`
          : 'waiting for a second sample'
      }
    >
      <div className="mt-2.5 space-y-1">
        <Flow label="↓ in" value={receiveBytesPerSec} />
        <Flow label="↑ out" value={transmitBytesPerSec} />
      </div>
      <p className="mt-2 truncate text-[10.5px] text-muted" title={busiest?.description}>
        {busiest ? busiest.name : 'No interface has a rate yet'}
      </p>
    </Card>
  );
}

/**
 * One direction of a throughput.
 *
 * The label is passed in rather than derived from an in/out flag, because a
 * disk does not receive and transmit — it reads and writes, and calling those
 * "in" and "out" would be borrowing the network's vocabulary for something
 * else.
 */
function Flow({ label, value }: { label: string; value: number | null }) {
  return (
    <div className="flex items-baseline justify-between font-mono text-[11.5px]">
      <span className="text-muted">{label}</span>
      <span className={value === null ? 'text-muted' : 'text-secondary tabular-nums'}>
        {value === null ? '—' : formatRate(value)}
      </span>
    </div>
  );
}
