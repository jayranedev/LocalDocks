import { ComingSoon } from '../components/ComingSoon';
import { PageHeader, PortBadge, Chip, StatusDot } from '../components/ui';

const DEV_ONLY = 'Hidden in production builds. Visible here because the DEV flag is on.';

type LogTone = 'accent' | 'success' | 'warning' | 'danger';

const LOG_TONES: Record<LogTone, string> = {
  accent: 'text-accent',
  success: 'text-success',
  warning: 'text-warning',
  danger: 'text-danger',
};

const LOG_LINES: Array<{ t: string; tag: string; tone: LogTone; msg: string }> = [
  { t: '14:48:09', tag: 'vite', tone: 'accent', msg: 'ready in 412 ms' },
  { t: '14:48:09', tag: 'vite', tone: 'accent', msg: 'Local:   http://localhost:5173/' },
  { t: '14:52:31', tag: 'hmr', tone: 'success', msg: 'update /src/routes/Cart.tsx' },
  { t: '14:52:31', tag: 'hmr', tone: 'success', msg: 'page reload src/main.tsx' },
  { t: '15:01:04', tag: 'warn', tone: 'warning', msg: 'chunk larger than 500 kB after minification' },
  { t: '15:03:47', tag: 'vite', tone: 'accent', msg: 'optimized dependencies changed, reloading' },
  { t: '15:03:48', tag: 'err', tone: 'danger', msg: 'Failed to resolve import "./legacy/api"' },
];

export function Logs() {
  return (
    <ComingSoon
      milestone="V3"
      status="EXPLORING"
      title="Logs"
      note={DEV_ONLY}
      preview={
        <>
          <PageHeader title="Logs" subtitle="Live stdout and stderr for a captured service." />
          <div className="mt-[18px] rounded-[9px] border border-border bg-surface px-[15px] py-[13px] font-mono text-[11.5px] leading-[1.85] text-secondary">
            {LOG_LINES.map((l, i) => (
              <div key={i}>
                <span className="text-muted">{l.t}</span>
                {'  '}
                <span className={LOG_TONES[l.tone]}>{l.tag}</span>
                {'  '}
                {l.msg}
              </div>
            ))}
          </div>
        </>
      }
    >
      Only possible for processes LocalDocks started itself — Windows gives no way to attach to an
      already-running process&rsquo;s stdout. Depends on service control landing first.
    </ComingSoon>
  );
}

const CONTAINERS = [
  { name: 'shopfront-db-1', image: 'postgres:16-alpine', port: 5432, mem: '421 MB', up: true },
  { name: 'shopfront-cache-1', image: 'redis:7-alpine', port: 6379, mem: '38 MB', up: true },
  { name: 'shopfront-mq-1', image: 'rabbitmq:3-management', port: 5672, mem: '112 MB', up: false },
];

export function Docker() {
  return (
    <ComingSoon
      milestone="V3"
      status="EXPLORING"
      title="Docker"
      note={DEV_ONLY}
      preview={
        <>
          <PageHeader title="Docker" subtitle="Containers, their published ports and resource usage." />
          <div className="mt-[18px] max-w-[760px] overflow-hidden rounded-[9px] border border-border bg-surface">
            {CONTAINERS.map((c) => (
              <div key={c.name} className="flex h-[46px] items-center gap-[13px] border-b border-border px-3.5 last:border-b-0">
                <StatusDot tone={c.up ? 'success' : 'muted'} />
                <span className="w-[170px] text-[12.5px]">{c.name}</span>
                <span className="w-[150px] font-mono text-[11px] text-muted">{c.image}</span>
                <PortBadge port={c.port} />
                <span className="flex-1" />
                <span className="font-mono text-[11.5px] text-secondary">{c.mem}</span>
              </div>
            ))}
          </div>
        </>
      }
    >
      Containers publish ports through a proxy process, so they surface in Services today as{' '}
      <span className="font-mono text-[11.5px]">com.docker.backend</span> rather than by name. This
      view would resolve them properly.
    </ComingSoon>
  );
}

const DISTROS = [
  { name: 'Ubuntu-24.04', version: 'WSL 2', ip: '172.24.118.3', up: true },
  { name: 'Debian', version: 'WSL 2', ip: 'stopped', up: false },
];

export function Wsl() {
  return (
    <ComingSoon
      milestone="V3"
      status="EXPLORING"
      title="WSL"
      note={DEV_ONLY}
      preview={
        <>
          <PageHeader title="WSL" subtitle="Distributions and the services running inside them." />
          <div className="mt-[18px] flex max-w-[640px] flex-col gap-[11px]">
            {DISTROS.map((d) => (
              <div key={d.name} className="rounded-[9px] border border-border bg-surface px-[15px] py-[13px]">
                <div className="flex items-center gap-2.5">
                  <StatusDot tone={d.up ? 'success' : 'muted'} />
                  <span className="text-[13px] font-medium">{d.name}</span>
                  <Chip>{d.version}</Chip>
                  <span className="flex-1" />
                  <span className="font-mono text-[11px] text-muted">{d.ip}</span>
                </div>
              </div>
            ))}
          </div>
        </>
      }
    >
      WSL2 runs in a separate network namespace, so its ports do not appear in the Windows TCP table
      unless forwarded. Needs a different discovery path entirely.
    </ComingSoon>
  );
}
