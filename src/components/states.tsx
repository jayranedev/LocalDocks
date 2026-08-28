import { Icon } from './Icon';
import { Button } from './ui';

/** First scan, before any snapshot has arrived. */
export function LoadingState() {
  return (
    <div className="px-6 py-[22px]">
      <div className="mb-5 flex items-center gap-[9px] text-[12.5px] text-t2">
        <span className="ld-spin size-[13px] rounded-full border-2 border-bd border-t-ac" />
        <span>Scanning local services…</span>
      </div>
      {[220, 170, 260, 190, 210].map((w, i) => (
        <div key={w} className="flex h-[46px] items-center gap-3.5 border-b border-bd">
          <span className="ld-skeleton size-2 rounded-full bg-skel" />
          <span className="ld-skeleton h-2.5 rounded bg-skel" style={{ width: w }} />
          <span className="flex-1" />
          <span className="ld-skeleton h-2 rounded bg-skel" style={{ width: 60 + (i % 3) * 22 }} />
          <span className="ld-skeleton h-2 w-14 rounded bg-skel" />
        </div>
      ))}
    </div>
  );
}

/** No developer services found — the common first-run case, not an error. */
export function EmptyState() {
  return (
    <div className="ld-fade-in flex flex-1 flex-col items-center justify-center gap-3.5 p-10">
      <span className="text-t3">
        <Icon name="plug" size={46} strokeWidth={1.2} />
      </span>
      <h2 className="text-[15px] font-medium">No development services running</h2>
      <p className="max-w-[400px] text-center text-[12.5px] leading-relaxed text-t2">
        LocalDocks watches for processes you own that are holding a listening socket. Start a dev
        server and it will appear here within a second.
      </p>
      <div className="mt-1 rounded-[7px] border border-bd bg-surf px-[11px] py-[7px]">
        <code className="font-mono text-[11.5px] text-t2">npm run dev</code>
      </div>
    </div>
  );
}

/** A scan failed. The last good snapshot stays visible elsewhere. */
export function ErrorState({
  message,
  detail,
  onRetry,
}: {
  message: string;
  detail: string;
  onRetry?: () => void;
}) {
  return (
    <div className="ld-fade-in flex flex-1 flex-col items-center justify-center gap-[13px] p-10">
      <span className="text-red">
        <Icon name="warn" size={40} strokeWidth={1.3} />
      </span>
      <h2 className="text-[15px] font-medium">{message}</h2>
      <pre className="max-w-[520px] rounded-[7px] border border-bd bg-surf px-[13px] py-2.5 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-t2">
        {detail}
      </pre>
      <p className="max-w-[420px] text-center text-[12.5px] leading-relaxed text-t2">
        No snapshot has been captured yet, so there is nothing to fall back to. LocalDocks will retry
        on the next tick.
      </p>
      {onRetry && (
        <Button icon="refresh" variant="primary" onClick={onRetry} className="mt-1">
          Retry now
        </Button>
      )}
    </div>
  );
}
