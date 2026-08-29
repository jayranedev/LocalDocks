import { useEffect, useState } from 'react';
import type { ProcessDetail } from '../types';
import { actionablePort, type DetailTarget } from '../lib/detail';
import { getProcessDetail, copyText, openExternal } from '../lib/ipc';
import {
  fieldText,
  formatBytes,
  formatClock,
  formatCpu,
  formatUptime,
  isDualStack,
  isFieldOk,
  localUrl,
} from '../lib/format';
import { RELEVANCE_LABELS } from '../lib/view';
import { Icon } from './Icon';
import { Button, Chip, IconButton, SectionLabel, StatusDot } from './ui';

/**
 * The one detail panel.
 *
 * Services, Processes and Ports all resolve their row to a DetailTarget and
 * render this. Nothing about it is service-specific — a Service is just a
 * process that happens to hold a socket.
 */
interface Props {
  target: DetailTarget;
  onClose: () => void;
  onTerminate: () => void;
}

export function ProcessDetailPanel({ target, onClose, onTerminate }: Props) {
  const [detail, setDetail] = useState<ProcessDetail | null>(null);
  const port = actionablePort(target);

  /* Tier-2 fetch. Fires when the panel opens, never on a sampler tick.
     The parent keys this component by processId, so a different process gets
     a fresh instance and there is no stale detail to clear. */
  useEffect(() => {
    let cancelled = false;
    void getProcessDetail(target.processId).then((d) => {
      if (!cancelled) setDetail(d);
    });
    return () => {
      cancelled = true;
    };
  }, [target.processId]);

  const metrics = [
    { k: 'CPU', v: formatCpu(target.cpuPercent) },
    { k: 'MEMORY', v: formatBytes(target.memoryBytes) },
    { k: 'UPTIME', v: formatUptime(target.uptimeSeconds) },
    { k: 'THREADS', v: String(target.threadCount) },
  ];

  return (
    <>
      <div
        className="ld-fade-in absolute inset-0"
        style={{ background: 'var(--scrim)' }}
        onClick={onClose}
      />
      <aside
        className="ld-slide-in absolute inset-y-0 right-0 flex w-[396px] flex-col border-l border-border bg-surface-raised"
        style={{ boxShadow: 'var(--shadow-panel)' }}
      >
        <div className="flex items-start gap-[11px] border-b border-border px-4 pt-4 pb-[13px]">
          <span className="mt-1.5">
            <StatusDot tone={target.isService ? 'success' : 'muted'} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="text-[14.5px] font-medium">{target.title}</h2>
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              {target.badge && <Chip tone="accent">{target.badge}</Chip>}
              <span className="font-mono text-[11px] text-muted">
                {target.processName} · PID {target.pid}
              </span>
            </div>
          </div>
          <IconButton name="close" label="Close" onClick={onClose} />
        </div>

        <div className="min-h-0 flex-1 overflow-auto px-4 py-[15px]">
          {target.endpoints.length > 0 && (
            <>
              <div className="mb-2">
                <SectionLabel>ENDPOINTS</SectionLabel>
              </div>
              <div className="mb-[17px] overflow-hidden rounded-lg border border-border bg-surface">
                {target.endpoints.map((e) => {
                  const active =
                    target.highlight?.port === e.port && target.highlight?.address === e.address;
                  return (
                    <div
                      key={`${e.protocol}-${e.address}-${e.port}`}
                      className={`flex h-[38px] items-center gap-[9px] border-b border-border px-3 last:border-b-0 ${
                        active ? 'bg-surface-selected' : ''
                      }`}
                    >
                      <span className="w-[34px] font-mono text-[11px] text-muted">{e.protocol}</span>
                      <span className="flex-1 font-mono text-[11.5px] text-primary">{e.address}</span>
                      {active && <Chip tone="quiet">selected</Chip>}
                      <span className="font-mono text-[11.5px] font-medium text-accent">{e.port}</span>
                    </div>
                  );
                })}
                {isDualStack(target.endpoints) && (
                  <p className="bg-surface-hover px-3 py-2 text-[11px] leading-snug text-muted">
                    Same PID on both stacks — grouped into one service.
                  </p>
                )}
              </div>
            </>
          )}

          {target.classification && (
            <>
              <div className="mb-2">
                <SectionLabel>CLASSIFICATION</SectionLabel>
              </div>
              {/* The reason, in full, is the accountability the registry owes
                  the user: a verdict they cannot check is one they cannot
                  correct. It names the rule that fired — a registry entry, a
                  matched signature, or the absence of both. */}
              <div className="mb-[17px] rounded-lg border border-border bg-surface px-3 py-[11px]">
                <Chip tone={target.classification.relevance === 'developer' ? 'accent' : 'quiet'}>
                  {RELEVANCE_LABELS[target.classification.relevance]}
                </Chip>
                <p className="mt-2 text-[11.5px] leading-[1.55] text-secondary">
                  {target.classification.reason}
                </p>
              </div>
            </>
          )}

          <div className="mb-2">
            <SectionLabel>RESOURCES</SectionLabel>
          </div>
          <div className="mb-[17px] grid grid-cols-2 gap-2">
            {metrics.map((m) => (
              <div key={m.k} className="rounded-lg border border-border bg-surface px-[11px] py-[9px]">
                <div className="text-[10px] tracking-[0.07em] text-muted">{m.k}</div>
                <div className="mt-1 font-mono text-[13px] text-primary tabular-nums">{m.v}</div>
              </div>
            ))}
          </div>

          <div className="mb-2 flex items-center gap-2">
            <SectionLabel>PROCESS</SectionLabel>
            <Chip tone="quiet">{detail ? 'fetched on open' : 'fetching…'}</Chip>
          </div>
          <div className="mb-[17px] overflow-hidden rounded-lg border border-border bg-surface">
            {detail ? (
              <>
                <Field label="Executable" field={detail.executable} />
                <Field label="Command line" field={detail.commandLine} />
                <Field label="Working directory" field={detail.workingDirectory} />
              </>
            ) : (
              [0, 1, 2].map((i) => (
                <div key={i} className="border-b border-border px-3 py-[9px] last:border-b-0">
                  <div className="ld-skeleton h-2 w-20 rounded bg-skeleton" />
                  <div className="ld-skeleton mt-2 h-2.5 w-full rounded bg-skeleton" />
                </div>
              ))
            )}
            <div className="grid grid-cols-2 border-t border-border">
              <div className="border-r border-border px-3 py-[9px]">
                <div className="mb-[3px] text-[10.5px] text-muted">Parent PID</div>
                <div className="font-mono text-[11.5px] text-secondary">{target.parentPid}</div>
              </div>
              <div className="px-3 py-[9px]">
                <div className="mb-[3px] text-[10.5px] text-muted">Started</div>
                <div className="font-mono text-[11.5px] text-secondary">{formatClock(target.startedAt)}</div>
              </div>
            </div>
          </div>

          <div className="flex gap-[7px]">
            <Button
              icon="external"
              className="flex-1"
              disabled={port === null}
              onClick={() => port !== null && void openExternal(localUrl(port))}
            >
              Open
            </Button>
            <Button
              icon="copy"
              className="flex-1"
              disabled={port === null}
              onClick={() => port !== null && void copyText(localUrl(port))}
            >
              Copy URL
            </Button>
            <Button icon="stop" variant="danger" onClick={onTerminate}>
              Terminate
            </Button>
          </div>

          {port === null && target.endpoints.length > 0 && (
            <p className="mt-2 text-[11px] leading-snug text-muted">
              No loopback TCP endpoint — nothing to open as a localhost URL.
            </p>
          )}
          {target.endpoints.length === 0 && (
            <p className="mt-2 text-[11px] leading-snug text-muted">
              This process holds no listening sockets.
            </p>
          )}
        </div>
      </aside>
    </>
  );
}

function Field({ label, field }: { label: string; field: ProcessDetail['executable'] }) {
  const ok = isFieldOk(field);
  return (
    <div className="border-b border-border px-3 py-[9px] last:border-b-0">
      <div className="mb-[3px] flex items-center gap-1.5 text-[10.5px] text-muted">
        {label}
        {field.kind === 'denied' && <Icon name="lock" size={11} />}
      </div>
      <div
        className={`font-mono text-[11.5px] leading-snug break-all ${ok ? 'text-secondary' : 'text-muted italic'}`}
      >
        {fieldText(field)}
      </div>
    </div>
  );
}
