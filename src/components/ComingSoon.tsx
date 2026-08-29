import type { ReactNode } from 'react';
import { Chip } from './ui';

/**
 * Wraps an unbuilt module's design.
 *
 * The design renders behind at reduced opacity rather than being replaced by
 * a grey rectangle — partly so the information architecture is proven to
 * accommodate the module, partly so it reads as planned rather than absent.
 */
export function ComingSoon({
  milestone,
  status,
  title,
  children,
  note,
  preview,
}: {
  milestone: string;
  status: 'PLANNED' | 'EXPLORING';
  title: string;
  children: ReactNode;
  note: string;
  preview: ReactNode;
}) {
  return (
    <div className="relative min-h-0 flex-1 overflow-hidden">
      <div className="pointer-events-none px-6 py-5 opacity-40" aria-hidden="true">
        {preview}
      </div>

      <div
        className="ld-pop-in absolute top-1/2 left-1/2 w-[452px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-strong bg-surface-raised px-[21px] py-[19px]"
        style={{ boxShadow: 'var(--shadow-panel)' }}
      >
        <Chip tone={status === 'PLANNED' ? 'future' : 'neutral'}>
          {status} · {milestone}
        </Chip>
        <h2 className="mt-[11px] text-[15px] font-medium">{title}</h2>
        <p className="mt-[7px] text-[12.5px] leading-relaxed text-secondary">{children}</p>
        <p className="mt-3 border-t border-border pt-[11px] text-[11px] leading-relaxed text-muted">{note}</p>
      </div>
    </div>
  );
}
