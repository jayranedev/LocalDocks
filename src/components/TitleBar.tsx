import type { AppMode } from '../types';
import { Logo } from './Icon';
import { Chip, IconButton, Kbd } from './ui';
import { Icon } from './Icon';
import { ModeSwitch } from './ModeSwitch';
import { useAppVersion } from '../hooks/useAppVersion';

interface TitleBarProps {
  onOpenPalette: () => void;
  onOpenSettings: () => void;
  mode: AppMode;
  onModeChange: (mode: AppMode) => void;
}

export function TitleBar({ onOpenPalette, onOpenSettings, mode, onModeChange }: TitleBarProps) {
  const version = useAppVersion();

  return (
    <header
      className="flex h-[46px] flex-none items-center gap-3 border-b border-border bg-surface-raised px-3.5"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-[9px]">
        <span className="text-accent">
          <Logo />
        </span>
        <span className="text-[13.5px] font-semibold tracking-[-0.01em]">LocalDocks</span>
        {version && <Chip tone="quiet">v{version}</Chip>}
      </div>

      <div className="flex-1" />

      {/* The mode switch is application chrome, not a screen control: it
          changes every screen at once, so it lives beside the app's identity
          rather than inside any one view. */}
      <ModeSwitch mode={mode} onChange={onModeChange} />

      <button
        type="button"
        onClick={onOpenPalette}
        className="flex h-7 items-center gap-2 rounded-md border border-border bg-surface pr-[9px] pl-2.5 text-muted transition-colors hover:border-border-strong hover:text-primary"
      >
        <Icon name="search" />
        <span className="text-xs">Search</span>
        <Kbd>Ctrl</Kbd>
        <Kbd>K</Kbd>
      </button>
      <IconButton name="gear" label="Settings" onClick={onOpenSettings} />
    </header>
  );
}
