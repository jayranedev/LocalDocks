import { useMemo, useState } from 'react';
import type { ProcessId, Service, Snapshot } from '../types';
import { formatBytes, formatCpu, formatUptime, isDualStack, primaryPort } from '../lib/format';
import { Icon } from '../components/Icon';
import {
  Chip,
  FilterChips,
  PageHeader,
  PortBadge,
  SearchInput,
  StatusDot,
  Th,
} from '../components/ui';

type SortKey = 'label' | 'processName' | 'pid' | 'cpu' | 'memory' | 'uptime';
type FilterId = 'all' | 'node' | 'python' | 'db';

const FILTERS: Array<{ id: FilterId; label: string; match: (s: Service) => boolean }> = [
  { id: 'all', label: 'All', match: () => true },
  { id: 'node', label: 'Node', match: (s) => s.processName.startsWith('node') },
  { id: 'python', label: 'Python', match: (s) => s.processName.startsWith('python') },
  { id: 'db', label: 'Databases', match: (s) => /postgres|redis|mysql|mongo/i.test(s.processName) },
];

const SORT_LABELS: Record<SortKey, string> = {
  label: 'name',
  processName: 'process',
  pid: 'PID',
  cpu: 'CPU',
  memory: 'memory',
  uptime: 'uptime',
};

interface Props {
  snapshot: Snapshot;
  selectedId: ProcessId | null;
  onSelect: (id: ProcessId) => void;
}

export function Services({ snapshot, selectedId, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('memory');
  const [filter, setFilter] = useState<FilterId>('all');
  const [query, setQuery] = useState('');

  const rows = useMemo(() => {
    const matcher = FILTERS.find((f) => f.id === filter)!.match;
    const q = query.trim().toLowerCase();

    return snapshot.services
      .filter(matcher)
      .filter((s) =>
        q
          ? `${s.label} ${s.processName} ${s.framework ?? ''} ${s.pid} ${s.endpoints
              .map((e) => e.port)
              .join(' ')}`
              .toLowerCase()
              .includes(q)
          : true,
      )
      .slice()
      .sort((a, b) => {
        switch (sort) {
          case 'label':
            return a.label.localeCompare(b.label);
          case 'processName':
            return a.processName.localeCompare(b.processName);
          case 'pid':
            return a.pid - b.pid;
          case 'cpu':
            return b.cpuPercent - a.cpuPercent;
          case 'uptime':
            return b.uptimeSeconds - a.uptimeSeconds;
          case 'memory':
          default:
            return b.memoryBytes - a.memoryBytes;
        }
      });
  }, [snapshot.services, sort, filter, query]);

  return (
    <div className="ld-fade-in flex min-h-0 flex-1 flex-col">
      <div className="px-6 pt-5">
        <PageHeader
          title="Services"
          subtitle="A process you own that is holding a listening socket on a non-system port."
        />

        <div className="mt-4 mb-3 flex items-center gap-2">
          <SearchInput value={query} onChange={setQuery} placeholder="Filter services…" />
          <FilterChips options={FILTERS} value={filter} onChange={setFilter} />
          <span className="flex-1" />
          <span className="text-[11.5px] text-muted">
            {rows.length} of {snapshot.services.length} · sorted by {SORT_LABELS[sort]}
          </span>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-6 pb-5">
        <div className="overflow-hidden rounded-[9px] border border-border bg-surface">
          <div className="flex h-[34px] items-center gap-[13px] border-b border-border bg-surface-hover px-3.5">
            <span className="w-2" />
            <Th width={186} onClick={() => setSort('label')} active={sort === 'label'}>
              SERVICE
            </Th>
            <Th width={120}>ENDPOINTS</Th>
            <span className="flex-1" />
            <Th width={96} onClick={() => setSort('processName')} active={sort === 'processName'}>
              PROCESS
            </Th>
            <Th width={52} onClick={() => setSort('pid')} active={sort === 'pid'}>
              PID
            </Th>
            <Th width={46} align="right" onClick={() => setSort('cpu')} active={sort === 'cpu'}>
              CPU
            </Th>
            <Th width={66} align="right" onClick={() => setSort('memory')} active={sort === 'memory'}>
              MEMORY
            </Th>
            <Th width={62} align="right" onClick={() => setSort('uptime')} active={sort === 'uptime'}>
              UPTIME
            </Th>
            <span className="w-[18px]" />
          </div>

          {rows.length === 0 ? (
            <p className="px-3.5 py-10 text-center text-[12.5px] text-muted">
              No services match the current search or filter.
            </p>
          ) : (
            rows.map((s) => (
              <ServiceRow key={s.id} service={s} selected={s.id === selectedId} onSelect={onSelect} />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function ServiceRow({
  service,
  selected,
  onSelect,
}: {
  service: Service;
  selected: boolean;
  onSelect: (id: ProcessId) => void;
}) {
  const port = primaryPort(service.endpoints);
  return (
    <button
      type="button"
      onClick={() => onSelect(service.id)}
      className={`group flex h-[50px] w-full items-center gap-[13px] border-b border-border px-3.5 text-left transition-colors last:border-b-0 hover:bg-surface-selected ${
        selected ? 'bg-surface-selected' : ''
      }`}
    >
      <StatusDot />
      <div className="w-[186px]">
        <div className="text-[13px] font-medium">{service.label}</div>
        <div className="mt-0.5 text-[11px] text-muted">{service.framework ?? service.processName}</div>
      </div>
      <div className="flex w-[120px] flex-wrap gap-1">
        {port !== null && <PortBadge port={port} />}
        {isDualStack(service.endpoints) && <Chip tone="quiet">v4+v6</Chip>}
      </div>
      <span className="flex-1" />
      <span className="w-24 font-mono text-[11.5px] text-muted">{service.processName}</span>
      <span className="w-[52px] font-mono text-[11.5px] text-muted tabular-nums">{service.pid}</span>
      <span className="w-[46px] text-right font-mono text-[11.5px] text-secondary tabular-nums">
        {formatCpu(service.cpuPercent)}
      </span>
      <span className="w-[66px] text-right font-mono text-[11.5px] text-primary tabular-nums">
        {formatBytes(service.memoryBytes)}
      </span>
      <span className="w-[62px] text-right font-mono text-[11.5px] text-muted tabular-nums">
        {formatUptime(service.uptimeSeconds)}
      </span>
      <span className="w-[18px] text-muted opacity-0 transition-opacity group-hover:opacity-100">
        <Icon name="chevron" size={14} />
      </span>
    </button>
  );
}
