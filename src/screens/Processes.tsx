import { useMemo, useState } from 'react';
import type { ProcessId, ProcessRow, Snapshot } from '../types';
import { formatBytes, formatCpu, formatUptime } from '../lib/format';
import { Icon } from '../components/Icon';
import {
  Chip,
  FilterChips,
  Note,
  PageHeader,
  SearchInput,
  StatusDot,
  Th,
} from '../components/ui';

type SortKey = 'name' | 'pid' | 'cpu' | 'memory' | 'threads' | 'uptime';
type FilterId = 'all' | 'services' | 'other';

const FILTERS: Array<{ id: FilterId; label: string; match: (p: ProcessRow) => boolean }> = [
  { id: 'all', label: 'All', match: () => true },
  { id: 'services', label: 'Services', match: (p) => p.isService },
  { id: 'other', label: 'Other', match: (p) => !p.isService },
];

const SORT_LABELS: Record<SortKey, string> = {
  name: 'name',
  pid: 'PID',
  cpu: 'CPU',
  memory: 'memory',
  threads: 'threads',
  uptime: 'uptime',
};

interface Props {
  snapshot: Snapshot;
  selectedId: ProcessId | null;
  onSelect: (id: ProcessId) => void;
}

export function Processes({ snapshot, selectedId, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('memory');
  const [filter, setFilter] = useState<FilterId>('all');
  const [query, setQuery] = useState('');

  const rows = useMemo(() => {
    const matcher = FILTERS.find((f) => f.id === filter)!.match;
    const q = query.trim().toLowerCase();

    return snapshot.processes
      .filter(matcher)
      .filter((p) => (q ? `${p.name} ${p.pid} ${p.parentPid}`.toLowerCase().includes(q) : true))
      .slice()
      .sort((a, b) => {
        switch (sort) {
          case 'name':
            return a.name.localeCompare(b.name) || a.pid - b.pid;
          case 'pid':
            return a.pid - b.pid;
          case 'cpu':
            return b.cpuPercent - a.cpuPercent;
          case 'threads':
            return b.threadCount - a.threadCount;
          case 'uptime':
            return b.uptimeSeconds - a.uptimeSeconds;
          case 'memory':
          default:
            return b.memoryBytes - a.memoryBytes;
        }
      });
  }, [snapshot.processes, sort, filter, query]);

  return (
    <div className="ld-fade-in flex min-h-0 flex-1 flex-col">
      <div className="px-6 pt-5">
        <PageHeader
          title="Processes"
          subtitle="Every process you own. Services filters this list down to the ones holding a socket."
        />

        <div className="mt-4 mb-3 flex items-center gap-2">
          <SearchInput value={query} onChange={setQuery} placeholder="Filter by name or PID…" />
          <FilterChips options={FILTERS} value={filter} onChange={setFilter} />
          <span className="flex-1" />
          <span className="text-[11.5px] text-t3">
            {rows.length} of {snapshot.processes.length} · sorted by {SORT_LABELS[sort]}
          </span>
        </div>

        <div className="mb-3">
          <Note icon="lock">
            System and other-user processes are excluded — LocalDocks never elevates.
          </Note>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-6 pb-5">
        <div className="overflow-hidden rounded-[9px] border border-bd bg-surf">
          <div className="flex h-[34px] items-center gap-[13px] border-b border-bd bg-surfhi px-3.5">
            <span className="w-2" />
            <Th width={192} onClick={() => setSort('name')} active={sort === 'name'}>
              NAME
            </Th>
            <Th width={60} onClick={() => setSort('pid')} active={sort === 'pid'}>
              PID
            </Th>
            <Th width={60}>PPID</Th>
            <span className="flex-1" />
            <Th width={56} align="right" onClick={() => setSort('cpu')} active={sort === 'cpu'}>
              CPU
            </Th>
            <Th width={70} align="right" onClick={() => setSort('memory')} active={sort === 'memory'}>
              MEMORY
            </Th>
            <Th width={62} align="right" onClick={() => setSort('threads')} active={sort === 'threads'}>
              THREADS
            </Th>
            <Th width={62} align="right" onClick={() => setSort('uptime')} active={sort === 'uptime'}>
              UPTIME
            </Th>
            <span className="w-[18px]" />
          </div>

          {rows.length === 0 ? (
            <p className="px-3.5 py-10 text-center text-[12.5px] text-t3">
              No processes match the current search or filter.
            </p>
          ) : (
            rows.map((p) => (
              <ProcessRowItem
                key={p.id}
                process={p}
                selected={p.id === selectedId}
                onSelect={onSelect}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function ProcessRowItem({
  process,
  selected,
  onSelect,
}: {
  process: ProcessRow;
  selected: boolean;
  onSelect: (id: ProcessId) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(process.id)}
      className={`group flex h-[42px] w-full items-center gap-[13px] border-b border-bd px-3.5 text-left transition-colors last:border-b-0 hover:bg-sel ${
        selected ? 'bg-sel' : ''
      }`}
    >
      <StatusDot tone={process.status === 'running' ? 'grn' : 't3'} />
      <span
        className={`flex w-[192px] items-center gap-1.5 font-mono text-[11.5px] ${
          process.isService ? 'text-t1' : 'text-t3'
        }`}
      >
        <span className="truncate">{process.name}</span>
        {process.isService && <Chip tone="quiet">svc</Chip>}
      </span>
      <span className="w-[60px] font-mono text-[11.5px] text-t3 tabular-nums">{process.pid}</span>
      <span className="w-[60px] font-mono text-[11.5px] text-t3 tabular-nums">
        {process.parentPid}
      </span>
      <span className="flex-1" />
      <span className="w-[56px] text-right font-mono text-[11.5px] text-t2 tabular-nums">
        {formatCpu(process.cpuPercent)}
      </span>
      <span className="w-[70px] text-right font-mono text-[11.5px] text-t1 tabular-nums">
        {formatBytes(process.memoryBytes)}
      </span>
      <span className="w-[62px] text-right font-mono text-[11.5px] text-t3 tabular-nums">
        {process.threadCount}
      </span>
      <span className="w-[62px] text-right font-mono text-[11.5px] text-t3 tabular-nums">
        {formatUptime(process.uptimeSeconds)}
      </span>
      <span className="w-[18px] text-t3 opacity-0 transition-opacity group-hover:opacity-100">
        <Icon name="chevron" size={14} />
      </span>
    </button>
  );
}
