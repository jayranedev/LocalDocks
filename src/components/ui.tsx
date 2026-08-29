import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

/* Small shared primitives. Kept in one file on purpose — a folder of
   twelve one-component files would be architecture for its own sake. */

export type ChipTone = 'neutral' | 'accent' | 'future' | 'quiet';

/**
 * Every tone names a role, not a colour. `future` is the one worth calling
 * out: it marks a module that is in the nav but has not shipped, and it is the
 * only place the theme's second accent is spent.
 */
const CHIP_TONES: Record<ChipTone, string> = {
  neutral: 'bg-surface-hover border-border text-muted',
  accent: 'border-transparent bg-accent-soft text-accent',
  future: 'border-transparent bg-accent-alt-soft text-accent-alt',
  quiet: 'bg-transparent border-border-strong text-muted',
};

export function Chip({ children, tone = 'neutral' }: { children: ReactNode; tone?: ChipTone }) {
  return (
    <span
      className={`inline-flex h-[18px] items-center rounded-[5px] border px-1.5 text-[10px] font-semibold tracking-[0.03em] whitespace-nowrap ${CHIP_TONES[tone]}`}
    >
      {children}
    </span>
  );
}

/** The port badge. The most-scanned element in the app, so it gets the accent. */
export function PortBadge({ port }: { port: number }) {
  return (
    <span
      className="inline-flex h-[21px] items-center rounded-[5px] bg-accent-soft px-[7px] font-mono text-[11.5px] font-medium text-accent"
    >
      :{port}
    </span>
  );
}

export type StatusTone = 'success' | 'muted' | 'danger' | 'warning';

/**
 * Tone -> tokens as a lookup rather than a template string. Building
 * `var(--${tone})` out of a prop means a typo renders an invisible element at
 * runtime instead of failing the build.
 */
const STATUS_TONES: Record<StatusTone, { dot: string; ring: string }> = {
  success: { dot: 'var(--success)', ring: 'var(--success-soft)' },
  muted: { dot: 'var(--text-muted)', ring: 'var(--muted-soft)' },
  danger: { dot: 'var(--danger)', ring: 'var(--danger-soft)' },
  warning: { dot: 'var(--warning)', ring: 'var(--warning-soft)' },
};

export function StatusDot({ tone = 'success' }: { tone?: StatusTone }) {
  const { dot, ring } = STATUS_TONES[tone];
  return (
    <span
      className="size-2 flex-none rounded-full"
      style={{ background: dot, boxShadow: `0 0 0 3px ${ring}` }}
    />
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex h-[17px] items-center rounded border border-border bg-background px-1 font-mono text-[10px] text-muted">
      {children}
    </span>
  );
}

export function Button({
  icon,
  children,
  onClick,
  variant = 'default',
  className = '',
  disabled,
}: {
  icon?: IconName;
  children?: ReactNode;
  onClick?: () => void;
  variant?: 'default' | 'primary' | 'danger' | 'dangerSolid';
  className?: string;
  disabled?: boolean;
}) {
  const variants = {
    default: 'border-border bg-surface text-secondary hover:border-border-strong hover:text-primary',
    primary: 'border-transparent bg-accent-soft text-accent',
    danger: 'border-border bg-surface text-danger hover:border-danger',
    dangerSolid: 'border-danger bg-danger text-danger-contrast hover:opacity-90',
  } as const;
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`ld-disabled flex h-8 items-center justify-center gap-[7px] rounded-[7px] border px-3 text-xs font-medium whitespace-nowrap transition-colors ${variants[variant]} ${className}`}
    >
      {icon && <Icon name={icon} />}
      {children && <span>{children}</span>}
    </button>
  );
}

export function IconButton({
  name,
  onClick,
  label,
}: {
  name: IconName;
  onClick?: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
      className="flex size-7 items-center justify-center rounded-md border border-transparent text-muted transition-colors hover:border-border-strong hover:text-primary"
    >
      <Icon name={name} />
    </button>
  );
}

export function PageHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div>
      <h1 className="text-[19px] font-semibold tracking-[-0.015em]">{title}</h1>
      <p className="mt-[5px] text-[12.5px] leading-relaxed text-secondary">{subtitle}</p>
    </div>
  );
}

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <span className="text-[10px] font-semibold tracking-[0.08em] text-muted">{children}</span>
  );
}

/** Icon tones for `Note`. Classes, so the token binding is checked at build. */
const NOTE_TONES: Record<StatusTone | 'accent', string> = {
  muted: 'text-muted',
  accent: 'text-accent',
  success: 'text-success',
  warning: 'text-warning',
  danger: 'text-danger',
};

export function Note({
  icon,
  tone = 'muted',
  children,
}: {
  icon: IconName;
  tone?: StatusTone | 'accent';
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-[9px] rounded-[7px] border border-border bg-surface px-3 py-[9px] text-[11.5px] leading-relaxed text-secondary">
      <span className={`flex-none ${NOTE_TONES[tone]}`}>
        <Icon name={icon} />
      </span>
      <span>{children}</span>
    </div>
  );
}

export function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <div className={`rounded-[9px] border border-border bg-surface ${className}`}>{children}</div>
  );
}

export function StatTile({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: string;
  sub: string;
  tone?: 'accent' | 'success';
}) {
  const toneClass = tone === 'accent' ? 'text-accent' : tone === 'success' ? 'text-success' : '';
  return (
    <div className="rounded-[9px] border border-border bg-surface px-[15px] py-[13px]">
      <div className="text-[10px] font-semibold tracking-[0.08em] text-muted">{label}</div>
      <div
        className={`my-[7px] text-[25px] font-semibold tracking-[-0.02em] tabular-nums ${toneClass}`}
      >
        {value}
      </div>
      <div className="text-[11px] text-muted">{sub}</div>
    </div>
  );
}

/** Sortable table header cell. */
export function Th({
  children,
  width,
  align = 'left',
  onClick,
  active,
}: {
  children: ReactNode;
  width?: number;
  align?: 'left' | 'right';
  onClick?: () => void;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!onClick}
      style={{ width, textAlign: align }}
      className={`text-[10px] font-semibold tracking-[0.07em] transition-colors ${
        active ? 'text-primary' : 'text-muted'
      } ${onClick ? 'cursor-pointer hover:text-primary' : 'cursor-default'}`}
    >
      {children}
      {active && <span className="ml-1">↓</span>}
    </button>
  );
}


export function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-5 border-t border-border py-[9px]">
      <div>
        <div className="text-[12.5px] text-primary">{label}</div>
        <div className="mt-[3px] text-[11px] leading-snug text-muted">{hint}</div>
      </div>
      {children}
    </div>
  );
}


/** Shared search box. One implementation for all three tables. */
export function SearchInput({
  value,
  onChange,
  placeholder,
  width = 260,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  width?: number;
}) {
  return (
    <div
      className="flex h-[30px] items-center gap-2 rounded-[7px] border border-border bg-surface px-2.5 text-muted focus-within:border-focus"
      style={{ width }}
    >
      <Icon name="search" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-transparent text-xs text-primary outline-none placeholder:text-muted"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange('')}
          aria-label="Clear search"
          className="text-muted hover:text-primary"
        >
          <Icon name="close" size={13} />
        </button>
      )}
    </div>
  );
}

/** Row of filter chips. Shared by Services and Ports. */
export function FilterChips<T extends string>({
  options,
  value,
  onChange,
}: {
  options: Array<{ id: T; label: string }>;
  value: T;
  onChange: (id: T) => void;
}) {
  return (
    <>
      {options.map((o) => {
        const active = value === o.id;
        return (
          <button
            key={o.id}
            type="button"
            onClick={() => onChange(o.id)}
            className={`flex h-[30px] items-center rounded-[7px] border px-[11px] text-xs transition-colors ${
              active
                ? 'border-transparent bg-accent-soft font-medium text-accent'
                : 'border-border bg-surface text-secondary hover:border-border-strong'
            }`}
          >
            {o.label}
          </button>
        );
      })}
    </>
  );
}
