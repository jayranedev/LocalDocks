import { useMemo, useState } from 'react';
import type { PortRow, ProcessId, Snapshot } from '../types';
import { Icon } from '../components/Icon';
import { FilterChips, Note, PageHeader, SearchInput, Th } from '../components/ui';

type SortKey = 'port' | 'protocol' | 'address' | 'pid' | 'process' | 'service';
type FilterId = 'all' | 'tcp' | 'udp';

const isV6 = (address: string) => address.startsWith('[');

const FILTERS: Array<{ id: FilterId; label: string; match: (r: PortRow) => boolean }> = [
  { id: 'all', label: 'All', match: () => true },
  { id: 'tcp', label: 'TCP', match: (r) => r.protocol === 'TCP' },
  { id: 'udp', label: 'UDP', match: (r) => r.protocol === 'UDP' },
];

const SORT_LABELS: Record<SortKey, string> = {
  port: 'port',
  protocol: 'protocol',
  address: 'address',
  pid: 'PID',
  process: 'process',
  service: 'service',
};

interface Props {
  snapshot: Snapshot;
  selectedId: ProcessId | null;
  onSelect: (id: ProcessId, endpoint: { port: number; address: string }) => void;
}

/**
 * Ports — the diagnostic view.
 *
 * Deliberately unmerged: one row per socket. Services groups by PID, which is
 * the right default, but when a port is behaving oddly you need to see that a
 * dev server is actually holding 127.0.0.1:5173 *and* [::1]:5173.
 */
export function Ports({ snapshot, selectedId, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('port');
  const [filter, setFilter] = useState<FilterId>('all');
  const [query, setQuery] = useState('');

  const rows = useMemo(() => {
    const matcher = FILTERS.find((f) => f.id === filter)!.match;
    const q = query.trim().toLowerCase();

    return snapshot.ports
      .filter(matcher)
      .filter((r) =>
        q
          ? `${r.port} ${r.protocol} ${r.address} ${r.pid} ${r.processName} ${r.serviceLabel ?? ''}`
              .toLowerCase()
              .includes(q)
          : true,
      )
      .slice()
      .sort((a, b) => {
        switch (sort) {
          case 'protocol':
            return a.protocol.localeCompare(b.protocol) || a.port - b.port;
          case 'address':
            return a.address.localeCompare(b.address) || a.port - b.port;
          case 'pid':
            return a.pid - b.pid || a.port - b.port;
          case 'process':
            return a.processName.localeCompare(b.processName) || a.port - b.port;
          case 'service':
            return (a.serviceLabel ?? '').localeCompare(b.serviceLabel ?? '') || a.port - b.port;
          case 'port':
          default:
            /* By port, then IPv4 before IPv6, so a dual-stack pair reads as
               "the real one, and its v6 twin" rather than in ASCII order. */
            return (
              a.port - b.port ||
              Number(isV6(a.address)) - Number(isV6(b.address)) ||
              a.address.localeCompare(b.address)
            );
        }
      });
  }, [snapshot.ports, sort, filter, query]);

  /* A second socket on the same port gets a tinted row, so dual-stack pairs
     read as one thing rather than two coincidences. Only meaningful while
     sorted by port. */
  const seen = new Set<number>();
  const tinted = new Map<PortRow, boolean>();
  for (const r of rows) {
    tinted.set(r, sort === 'port' && seen.has(r.port));
    seen.add(r.port);
  }

  return (
    <div className="ld-fade-in flex min-h-0 flex-1 flex-col">
      <div className="px-6 pt-5">
        <PageHeader title="Ports" subtitle="One row per socket, unmerged. This is the diagnostic view." />

        <div className="mt-4 mb-3 flex items-center gap-2">
          <SearchInput value={query} onChange={setQuery} placeholder="Filter by port, PID or process…" />
          <FilterChips options={FILTERS} value={filter} onChange={setFilter} />
          <span className="flex-1" />
          <span className="text-[11.5px] text-muted">
            {rows.length} of {snapshot.ports.length} · sorted by {SORT_LABELS[sort]}
          </span>
        </div>

        <div className="mb-3">
          <Note icon="warn" tone="accent">
            Dev servers commonly bind both IPv4 and IPv6. Services groups these by PID — here they stay
            separate on purpose.
          </Note>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-6 pb-5">
        <div className="overflow-hidden rounded-[9px] border border-border bg-surface">
          <div className="flex h-[34px] items-center gap-[13px] border-b border-border bg-surface-hover px-3.5">
            <Th width={62} onClick={() => setSort('port')} active={sort === 'port'}>
              PORT
            </Th>
            <Th width={58} onClick={() => setSort('protocol')} active={sort === 'protocol'}>
              PROTO
            </Th>
            <Th width={150} onClick={() => setSort('address')} active={sort === 'address'}>
              ADDRESS
            </Th>
            <Th width={60} onClick={() => setSort('pid')} active={sort === 'pid'}>
              PID
            </Th>
            <Th width={118} onClick={() => setSort('process')} active={sort === 'process'}>
              PROCESS
            </Th>
            <span className="flex-1" />
            <Th width={130} onClick={() => setSort('service')} active={sort === 'service'}>
              SERVICE
            </Th>
            <Th width={74} align="right">
              STATE
            </Th>
            <span className="w-[18px]" />
          </div>

          {rows.length === 0 ? (
            <p className="px-3.5 py-10 text-center text-[12.5px] text-muted">
              No ports match the current search or filter.
            </p>
          ) : (
            rows.map((r) => {
              const selectable = r.processId !== null;
              return (
                <button
                  key={`${r.protocol}-${r.address}-${r.port}-${r.pid}`}
                  type="button"
                  disabled={!selectable}
                  onClick={() =>
                    r.processId && onSelect(r.processId, { port: r.port, address: r.address })
                  }
                  title={selectable ? undefined : 'Owning process could not be identified'}
                  className={`group flex h-[38px] w-full items-center gap-[13px] border-b border-border px-3.5 text-left transition-colors last:border-b-0 ${
                    selectable ? 'cursor-pointer hover:bg-surface-selected' : 'cursor-default'
                  } ${r.processId === selectedId ? 'bg-surface-selected' : tinted.get(r) ? 'bg-surface-hover' : ''}`}
                >
                  <span className="w-[62px] font-mono text-xs font-medium text-accent tabular-nums">
                    {r.port}
                  </span>
                  <span className="w-[58px] font-mono text-[11.5px] text-muted">{r.protocol}</span>
                  <span className="w-[150px] font-mono text-[11.5px] text-secondary">
                    {r.address}:{r.port}
                  </span>
                  <span className="w-[60px] font-mono text-[11.5px] text-muted tabular-nums">{r.pid}</span>
                  <span className="w-[118px] font-mono text-[11.5px] text-muted">{r.processName}</span>
                  <span className="flex-1" />
                  <span className="w-[130px] text-xs text-primary">{r.serviceLabel ?? '—'}</span>
                  <span className="w-[74px] text-right text-[11px] text-success">{r.state}</span>
                  <span className="w-[18px] text-muted opacity-0 transition-opacity group-hover:opacity-100">
                    {selectable && <Icon name="chevron" size={14} />}
                  </span>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
