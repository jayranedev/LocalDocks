import type { AppMode, Service, Snapshot } from '../types';
import { formatBytes, formatCpu, isDualStack, primaryPort } from '../lib/format';
import type { SnapshotView } from '../lib/view';
import { MODE_HINTS, MODE_LABELS, RELEVANCE_LABELS } from '../lib/view';
import { Icon } from '../components/Icon';
import { TelemetryCards } from '../components/TelemetryCards';
import { Chip, PageHeader, PortBadge, SectionLabel, StatTile, StatusDot } from '../components/ui';

interface Props {
  snapshot: Snapshot;
  view: SnapshotView;
  intervalMs: number;
  onSelect: (id: string) => void;
}

export function Overview({ snapshot, view, intervalMs, onSelect }: Props) {
  const { services, ports, conflicts } = snapshot;

  const totalMemory = services.reduce((sum, s) => sum + s.memoryBytes, 0);
  const totalCpu = services.reduce((sum, s) => sum + s.cpuPercent, 0);
  const dualStackCount = services.filter((s) => isDualStack(s.endpoints)).length;
  const distinctPorts = new Set(ports.map((p) => p.port)).size;

  return (
    <div className="ld-fade-in overflow-auto px-6 py-5">
      {/* The subtitle has to change with the mode: in Developer mode this is
          deliberately not everything, and saying otherwise would misdescribe
          the very narrowing the mode exists to do. */}
      <PageHeader
        title="Overview"
        subtitle={
          view.mode === 'developer'
            ? 'Your classified development services, and what they are holding.'
            : 'Everything you own that is listening on this machine.'
        }
      />

      {/* What is being shown, and on whose authority. Every number below is
          the current mode's view; the mode and the sampler cadence say where
          those numbers came from. */}
      <div className="mt-3 flex flex-wrap items-center gap-x-2.5 gap-y-1.5 text-[11.5px] text-muted">
        <Chip tone={view.mode === 'developer' ? 'accent' : 'neutral'}>
          {MODE_LABELS[view.mode]}
        </Chip>
        <span>{MODE_HINTS[view.mode]}</span>
        <span>·</span>
        <span className="font-mono">sampler {intervalMs} ms</span>
        <span>·</span>
        {/* What the tick actually cost. Published rather than logged because
            the price of a monitoring tool is the user's business, and because
            a telemetry provider that becomes slow on some other machine should
            be visible without a debug build. */}
        <span
          className="font-mono tabular-nums"
          title={`processes ${snapshot.timing.processesMillis.toFixed(1)} ms · sockets ${snapshot.timing.portsMillis.toFixed(1)} ms · telemetry ${snapshot.timing.telemetryMillis.toFixed(1)} ms`}
        >
          tick {snapshot.timing.totalMillis.toFixed(0)} ms
        </span>
        <span>·</span>
        <span className="tabular-nums">
          {services.length} of {view.total.services} services
        </span>
        <span>·</span>
        <span className="tabular-nums">
          {snapshot.processes.length} of {view.total.processes} processes
        </span>
        <span>·</span>
        <span className="tabular-nums">
          {snapshot.ports.length} of {view.total.ports} sockets
        </span>
      </div>

      <div className="mt-[18px] mb-[22px] grid grid-cols-4 gap-3">
        <StatTile
          label="SERVICES"
          value={String(services.length)}
          sub={`${new Set(services.map((s) => s.processName)).size} distinct runtimes`}
          tone="accent"
        />
        <StatTile
          label="LISTENING PORTS"
          value={String(distinctPorts)}
          sub={`${ports.length} sockets, ${dualStackCount} dual-stack`}
        />
        {/* null until the backend computes it — "—", never a confident 0. */}
        <StatTile
          label="CONFLICTS"
          value={conflicts === null ? '—' : String(conflicts)}
          sub={
            conflicts === null
              ? 'detection not implemented'
              : conflicts === 0
                ? 'no duplicate binds'
                : 'ports bound twice'
          }
          tone={conflicts === 0 ? 'success' : undefined}
        />
        {/* Explicitly "across services", not machine memory — the machine's
            own figures are in the System strip below, where they cannot be
            confused with this sum. */}
        <StatTile
          label="SERVICE MEMORY"
          value={formatBytes(totalMemory)}
          sub={`${formatCpu(totalCpu)} CPU across services`}
        />
      </div>

      <div className="mb-[9px] flex items-baseline gap-2.5">
        <SectionLabel>RUNNING</SectionLabel>
        <span className="h-px flex-1 bg-border" />
      </div>

      <div className="overflow-hidden rounded-[9px] border border-border bg-surface">
        {services.map((s) => (
          <OverviewRow key={s.id} service={s} mode={view.mode} onSelect={onSelect} />
        ))}
      </div>

      <TelemetryCards system={snapshot.system} />
    </div>
  );
}

function OverviewRow({
  service,
  mode,
  onSelect,
}: {
  service: Service;
  mode: AppMode;
  onSelect: (id: string) => void;
}) {
  const port = primaryPort(service.endpoints);
  return (
    <button
      type="button"
      onClick={() => onSelect(service.id)}
      className="group flex h-[52px] w-full items-center gap-[13px] border-b border-border px-3.5 text-left transition-colors last:border-b-0 hover:bg-surface-selected"
      /* The reason is the whole point of a registry: a classification the user
         cannot check is one they cannot correct. */
      title={service.relevanceReason}
    >
      <StatusDot />
      <div className="w-[186px]">
        <div className="text-[13px] font-medium">{service.label}</div>
        <div className="mt-0.5 text-[11px] text-muted">{service.framework ?? service.processName}</div>
      </div>
      {port !== null && <PortBadge port={port} />}
      {/* In Developer mode every row is a developer service, so the chip would
          say the same thing on every line. It earns its place only where the
          list is mixed. */}
      {mode === 'system' && (
        <Chip tone={service.relevance === 'developer' ? 'accent' : 'quiet'}>
          {RELEVANCE_LABELS[service.relevance]}
        </Chip>
      )}
      <span className="flex-1" />
      <span className="w-24 font-mono text-[11.5px] text-muted">{service.processName}</span>
      <span className="w-[52px] font-mono text-[11.5px] text-muted tabular-nums">{service.pid}</span>
      <span className="w-[46px] text-right font-mono text-[11.5px] text-secondary tabular-nums">
        {formatCpu(service.cpuPercent)}
      </span>
      <span className="w-[66px] text-right font-mono text-[11.5px] text-primary tabular-nums">
        {formatBytes(service.memoryBytes)}
      </span>
      <span className="w-[18px] text-muted opacity-0 transition-opacity group-hover:opacity-100">
        <Icon name="chevron" size={14} />
      </span>
    </button>
  );
}
