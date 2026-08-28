import { ComingSoon } from '../components/ComingSoon';
import { Icon } from '../components/Icon';
import { Chip, PageHeader, PortBadge } from '../components/ui';

/** Illustrative shape only — real grouping needs tier-2 working directories. */
const SAMPLE = [
  {
    name: 'shopfront',
    branch: 'feat/checkout',
    path: 'C:\\dev\\shopfront',
    items: [
      { label: 'Frontend', port: 5173, mem: '142 MB' },
      { label: 'API', port: 8000, mem: '318 MB' },
      { label: 'Worker', port: 8001, mem: '126 MB' },
    ],
  },
  {
    name: 'infrastructure',
    branch: 'main',
    path: 'C:\\dev\\infra',
    items: [
      { label: 'PostgreSQL', port: 5432, mem: '421 MB' },
      { label: 'Redis', port: 6379, mem: '38 MB' },
    ],
  },
];

export function Projects() {
  return (
    <ComingSoon
      milestone="V2"
      status="PLANNED"
      title="Project awareness"
      note="Requires tier-2 command line and working directory — fetched on demand, never in the scan loop."
      preview={
        <>
          <PageHeader
            title="Projects"
            subtitle="Services grouped by the project directory they were launched from."
          />
          <div className="mt-5 flex max-w-[640px] flex-col gap-3.5">
            {SAMPLE.map((p) => (
              <div key={p.name} className="overflow-hidden rounded-[9px] border border-bd bg-surf">
                <div className="flex items-center gap-2.5 border-b border-bd px-3.5 py-[11px]">
                  <span className="text-ac">
                    <Icon name="folder" />
                  </span>
                  <span className="text-[13px] font-medium">{p.name}</span>
                  <Chip>{p.branch}</Chip>
                  <span className="flex-1" />
                  <span className="font-mono text-[11px] text-t3">{p.path}</span>
                </div>
                {p.items.map((it, i) => (
                  <div
                    key={it.label}
                    className="flex h-10 items-center gap-3 border-b border-bd pr-3.5 pl-[22px] last:border-b-0"
                  >
                    <span className="font-mono text-[11px] text-t3">
                      {i === p.items.length - 1 ? '└─' : '├─'}
                    </span>
                    <span className="w-[104px] text-[12.5px]">{it.label}</span>
                    <PortBadge port={it.port} />
                    <span className="flex-1" />
                    <span className="font-mono text-[11.5px] text-t3">{it.mem}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </>
      }
    >
      Groups services by working directory and Git repository, so a frontend, an API and a worker read
      as one project rather than three unrelated processes.
    </ComingSoon>
  );
}
