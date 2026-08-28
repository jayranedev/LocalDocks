import { Logo } from './Icon';
import { Chip, IconButton, Kbd } from './ui';
import { Icon } from './Icon';

interface TitleBarProps {
  onOpenPalette: () => void;
  onOpenSettings: () => void;
}

export function TitleBar({ onOpenPalette, onOpenSettings }: TitleBarProps) {
  return (
    <header
      className="flex h-[46px] flex-none items-center gap-3 border-b border-bd bg-elev px-3.5"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-[9px]">
        <span className="text-ac">
          <Logo />
        </span>
        <span className="text-[13.5px] font-semibold tracking-[-0.01em]">LocalDocks</span>
        <Chip tone="quiet">v0.1.0</Chip>
      </div>

      <div className="flex-1" />

      <button
        type="button"
        onClick={onOpenPalette}
        className="flex h-7 items-center gap-2 rounded-md border border-bd bg-surf pr-[9px] pl-2.5 text-t3 transition-colors hover:border-bdhi hover:text-t1"
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
