import { useEffect, useRef, useState } from 'react';
import type { TerminateResult } from '../types';
import type { DetailTarget } from '../lib/detail';
import { terminateProcess } from '../lib/ipc';
import { formatClock } from '../lib/format';
import { Icon } from './Icon';
import { Button } from './ui';

interface Props {
  target: DetailTarget;
  onClose: () => void;
  onTerminated: () => void;
}

/**
 * Force-terminate confirmation.
 *
 * The copy states the two things a developer needs to know and most tools
 * hide: that the PID is re-verified against its creation time before anything
 * is killed, and that this is a hard kill because Windows has no SIGTERM.
 *
 * There is no "don't ask again" — a safety confirmation you can switch off is
 * not a safety confirmation.
 */
export function TerminateDialog({ target, onClose, onTerminated }: Props) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<TerminateResult | null>(null);
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  /* Move focus into the dialog, and put it back where it came from on close.
     `aria-modal` tells a screen reader this is modal; it does not move focus,
     and without that a keyboard user's focus stays on the row behind — so the
     dialog is announced to nobody and Tab walks the page underneath it. The
     opener is remembered rather than assumed, because this dialog is reachable
     from the detail panel and from the process table. */
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    /* Cancel, not Force terminate: the destructive action should never be the
       one a stray Enter reaches first. */
    const cancel = panel.current?.querySelector<HTMLElement>('[data-autofocus]');
    (cancel ?? panel.current)?.focus();
    return () => opener?.focus?.();
  }, []);

  async function confirm() {
    setBusy(true);
    const r = await terminateProcess({ pid: target.pid, startedAt: target.startedAt });
    setBusy(false);
    setResult(r);
    if (r.kind === 'terminated') onTerminated();
  }

  return (
    <div
      className="ld-fade-in absolute inset-0 flex items-center justify-center"
      style={{ background: 'var(--scrim)' }}
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={`Terminate ${target.title}`}
    >
      <div
        ref={panel}
        tabIndex={-1}
        className="ld-pop-in w-[436px] rounded-xl border border-border-strong bg-surface-raised px-5 py-[19px]"
        style={{ boxShadow: 'var(--shadow-panel)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-[13px] flex items-center gap-2.5">
          <span className="text-danger">
            <Icon name="warn" />
          </span>
          <h2 className="text-[14.5px] font-medium">Terminate {target.title}?</h2>
        </div>

        <div className="mb-[13px] rounded-[7px] border border-border bg-surface px-[11px] py-[9px] font-mono text-[11.5px] leading-relaxed text-secondary">
          {target.processName} &nbsp;·&nbsp; PID {target.pid}
          <br />
          started {formatClock(target.startedAt)}
        </div>

        <div className="mb-3 flex gap-[9px] rounded-[7px] border border-border bg-accent-soft px-3 py-2.5">
          <span className="mt-px text-accent">
            <Icon name="lock" />
          </span>
          <p className="text-[11.5px] leading-relaxed text-secondary">
            LocalDocks re-opens the process and checks its creation time matches before terminating.
            If Windows has recycled PID {target.pid}, the action is refused.
          </p>
        </div>

        <p className="mb-4 text-[11.5px] leading-relaxed text-secondary">
          This is a <strong className="font-semibold text-primary">force terminate</strong>. The process
          gets no chance to clean up and unsaved work is lost. Windows has no graceful equivalent of
          SIGTERM.
        </p>

        {result && result.kind !== 'terminated' && (
          <div className="mb-3 rounded-[7px] border border-danger bg-danger-soft px-3 py-2.5 text-[11.5px] leading-relaxed text-danger">
            {result.kind === 'stale' && result.message}
            {result.kind === 'denied' && 'Access denied. This process is owned by another account.'}
            {result.kind === 'failed' && result.message}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <Button onClick={onClose} autoFocusTarget>
            Cancel
          </Button>
          <Button icon="stop" variant="dangerSolid" onClick={() => void confirm()} disabled={busy}>
            {busy ? 'Verifying…' : 'Force terminate'}
          </Button>
        </div>
      </div>
    </div>
  );
}
