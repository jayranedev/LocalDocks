import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

/* Small shared primitives. Kept in one file on purpose — a folder of
   twelve one-component files would be architecture for its own sake. */

export function Chip({
  children,
  tone = 'neutral',
}: {
  children: ReactNode;
  tone?: 'neutral' | 'accent' | 'quiet';
}) {
  const tones = {
    neutral: 'bg-surfhi border-bd text-t3',
    accent: 'border-transparent text-ac',
    quiet: 'bg-transparent border-bd text-t3',
  } as const;
  return (
    <span
      className={`inline-flex h-[18px] items-center rounded-[5px] border px-1.5 text-[10px] font-semibold tracking-[0.03em] whitespace-nowrap ${tones[tone]}`}
      style={tone === 'accent' ? { background: 'color-mix(in srgb, var(--c-ac) 14%, transparent)' } : undefined}
    >
      {children}
    </span>
  );
}

/** The port badge. The most-scanned element in the app, so it gets the accent. */
export function PortBadge({ port }: { port: number }) {
  return (
    <span
      className="inline-flex h-[21px] items-center rounded-[5px] px-[7px] font-mono text-[11.5px] font-medium text-ac"
      style={{ background: 'color-mix(in srgb, var(--c-ac) 14%, transparent)' }}
    >
      :{port}
    </span>
  );
}

export function StatusDot({ tone = 'grn' }: { tone?: 'grn' | 't3' | 'red' | 'amb' }) {
  const color = `var(--c-${tone})`;
  return (
    <span
      className="size-2 flex-none rounded-full"
      style={{ background: color, boxShadow: `0 0 0 3px color-mix(in srgb, ${color} 18%, transparent)` }}
    />
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex h-[17px] items-center rounded border border-bd bg-bg px-1 font-mono text-[10px] text-t3">
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
    default: 'border-bd bg-surf text-t2 hover:border-bdhi hover:text-t1',
    primary: 'border-transparent text-ac',
    danger: 'border-bd bg-surf text-red hover:border-red',
    dangerSolid: 'border-red bg-red text-white hover:opacity-90',
  } as const;
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`flex h-8 items-center justify-center gap-[7px] rounded-[7px] border px-3 text-xs font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-45 ${variants[variant]} ${className}`}
      style={
        variant === 'primary'
          ? { background: 'color-mix(in srgb, var(--c-ac) 14%, transparent)' }
          : undefined
      }
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
      className="flex size-7 items-center justify-center rounded-md border border-transparent text-t3 transition-colors hover:border-bdhi hover:text-t1"
    >
      <Icon name={name} />
    </button>
  );
}

export function PageHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div>
      <h1 className="text-[19px] font-semibold tracking-[-0.015em]">{title}</h1>
      <p className="mt-[5px] text-[12.5px] leading-relaxed text-t2">{subtitle}</p>
    </div>
  );
}

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <span className="text-[10px] font-semibold tracking-[0.08em] text-t3">{children}</span>
  );
}

export function Note({ icon, tone = 't3', children }: { icon: IconName; tone?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-[9px] rounded-[7px] border border-bd bg-surf px-3 py-[9px] text-[11.5px] leading-relaxed text-t2">
      <span style={{ color: `var(--c-${tone})` }} className="flex-none">
        <Icon name={icon} />
      </span>
      <span>{children}</span>
    </div>
  );
}

export function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <div className={`rounded-[9px] border border-bd bg-surf ${className}`}>{children}</div>
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
  tone?: 'ac' | 'grn';
}) {
  return (
    <div className="rounded-[9px] border border-bd bg-surf px-[15px] py-[13px]">
      <div className="text-[10px] font-semibold tracking-[0.08em] text-t3">{label}</div>
      <div
        className="my-[7px] text-[25px] font-semibold tracking-[-0.02em] tabular-nums"
        style={tone ? { color: `var(--c-${tone})` } : undefined}
      >
        {value}
      </div>
      <div className="text-[11px] text-t3">{sub}</div>
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
        active ? 'text-t1' : 'text-t3'
      } ${onClick ? 'cursor-pointer hover:text-t1' : 'cursor-default'}`}
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
    <div className="flex items-center justify-between gap-5 border-t border-bd py-[9px]">
      <div>
        <div className="text-[12.5px] text-t1">{label}</div>
        <div className="mt-[3px] text-[11px] leading-snug text-t3">{hint}</div>
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
      className="flex h-[30px] items-center gap-2 rounded-[7px] border border-bd bg-surf px-2.5 text-t3 focus-within:border-bdhi"
      style={{ width }}
    >
      <Icon name="search" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-transparent text-xs text-t1 outline-none placeholder:text-t3"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange('')}
          aria-label="Clear search"
          className="text-t3 hover:text-t1"
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
                ? 'border-transparent font-medium text-ac'
                : 'border-bd bg-surf text-t2 hover:border-bdhi'
            }`}
            style={active ? { background: 'color-mix(in srgb, var(--c-ac) 14%, transparent)' } : undefined}
          >
            {o.label}
          </button>
        );
      })}
    </>
  );
}
