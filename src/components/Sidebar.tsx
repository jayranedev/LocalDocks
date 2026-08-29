import type { ScreenId } from '../types';
import { IS_DEV, MODULES, isVisible } from '../config/flags';
import { Icon, type IconName } from './Icon';
import { Chip } from './ui';

interface NavEntry {
  id: ScreenId;
  label: string;
  icon: IconName;
}

/** Shipped modules. Always in the nav. */
const PRIMARY: NavEntry[] = [
  { id: 'overview', label: 'Overview', icon: 'grid' },
  { id: 'services', label: 'Services', icon: 'layers' },
  { id: 'processes', label: 'Processes', icon: 'cpu' },
  { id: 'ports', label: 'Ports', icon: 'plug' },
];

/** Committed but unbuilt. Shown with its milestone. */
const PREVIEW: NavEntry[] = [{ id: 'projects', label: 'Projects', icon: 'folder' }];

/** Exploratory. Dev builds only — see config/flags.ts. */
const LATER: NavEntry[] = [
  { id: 'logs', label: 'Logs', icon: 'terminal' },
  { id: 'docker', label: 'Docker', icon: 'box' },
  { id: 'wsl', label: 'WSL', icon: 'wsl' },
];

interface SidebarProps {
  screen: ScreenId;
  onNavigate: (id: ScreenId) => void;
  counts: Partial<Record<ScreenId, number>>;
}

export function Sidebar({ screen, onNavigate, counts }: SidebarProps) {
  const item = (entry: NavEntry) => {
    if (!isVisible(entry.id)) return null;
    const active = screen === entry.id;
    const milestone = MODULES[entry.id].milestone;
    const count = counts[entry.id];

    return (
      <button
        key={entry.id}
        type="button"
        onClick={() => onNavigate(entry.id)}
        aria-current={active ? 'page' : undefined}
        className={`mb-px flex h-[31px] w-full items-center gap-2.5 rounded-md px-2.5 text-[12.5px] transition-colors ${
          active ? 'bg-surface-selected font-medium text-primary' : 'text-secondary hover:bg-surface-selected'
        }`}
      >
        <Icon name={entry.icon} />
        <span className="flex-1 text-left">{entry.label}</span>
        {milestone ? (
          <Chip tone="future">{milestone}</Chip>
        ) : count !== undefined ? (
          <span className="font-mono text-[10.5px] text-muted tabular-nums">{count}</span>
        ) : null}
      </button>
    );
  };

  const showLater = IS_DEV && LATER.some((e) => isVisible(e.id));

  return (
    <nav className="flex w-[212px] flex-none flex-col border-r border-border bg-surface-raised px-2 py-2.5">
      {PRIMARY.map(item)}
      {PREVIEW.map(item)}

      {showLater && (
        <>
          <div className="mt-3.5 mb-1.5 flex items-center gap-2 px-2.5">
            <span className="text-[10px] font-semibold tracking-[0.09em] text-muted">LATER</span>
            <span className="h-px flex-1 bg-border" />
            <Chip tone="quiet">DEV</Chip>
          </div>
          {LATER.map(item)}
        </>
      )}

      <div className="flex-1" />
      {item({ id: 'settings', label: 'Settings', icon: 'gear' })}
    </nav>
  );
}
