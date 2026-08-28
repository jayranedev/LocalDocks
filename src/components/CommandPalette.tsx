import { useEffect, useMemo, useRef, useState } from 'react';
import type { ScreenId, Service } from '../types';
import { copyText, openExternal } from '../lib/ipc';
import { localUrl, primaryPort } from '../lib/format';
import { Icon } from './Icon';
import { Kbd } from './ui';

interface Command {
  id: string;
  label: string;
  hint: string;
  run: () => void;
}

interface Props {
  services: Service[];
  onClose: () => void;
  onNavigate: (id: ScreenId) => void;
  onSelectService: (id: string) => void;
}

export function CommandPalette({ services, onClose, onNavigate, onSelectService }: Props) {
  const [query, setQuery] = useState('');
  const [rawCursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => inputRef.current?.focus(), []);

  const commands = useMemo<Command[]>(() => {
    const nav: Command[] = (
      [
        ['overview', 'Go to Overview'],
        ['services', 'Go to Services'],
        ['processes', 'Go to Processes'],
        ['ports', 'Go to Ports'],
        ['settings', 'Open settings'],
      ] as Array<[ScreenId, string]>
    ).map(([id, label]) => ({
      id: `nav-${id}`,
      label,
      hint: '',
      run: () => {
        onNavigate(id);
        onClose();
      },
    }));

    const perService = services.flatMap((s) => {
      const port = primaryPort(s.endpoints);
      const items: Command[] = [
        {
          id: `show-${s.id}`,
          label: `Show ${s.label}`,
          hint: port ? `:${port}` : s.processName,
          run: () => {
            onNavigate('services');
            onSelectService(s.id);
            onClose();
          },
        },
      ];
      if (port !== null) {
        items.push(
          {
            id: `open-${s.id}`,
            label: `Open localhost:${port} in browser`,
            hint: s.label,
            run: () => {
              void openExternal(localUrl(port));
              onClose();
            },
          },
          {
            id: `copy-${s.id}`,
            label: `Copy localhost:${port}`,
            hint: s.label,
            run: () => {
              void copyText(localUrl(port));
              onClose();
            },
          },
        );
      }
      return items;
    });

    return [...perService, ...nav];
  }, [services, onClose, onNavigate, onSelectService]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands.slice(0, 8);
    return commands
      .filter((c) => `${c.label} ${c.hint}`.toLowerCase().includes(q))
      .slice(0, 8);
  }, [commands, query]);

  /* Clamp rather than reset-on-change: the list shrinks as the user types, and
     deriving the valid cursor here avoids a second render pass. */
  const cursor = Math.min(rawCursor, Math.max(0, filtered.length - 1));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') return onClose();
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setCursor((c) => Math.min(c + 1, filtered.length - 1));
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setCursor((c) => Math.max(c - 1, 0));
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        filtered[cursor]?.run();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [filtered, cursor, onClose]);

  return (
    <div
      className="ld-fade-in absolute inset-0 flex items-start justify-center pt-24"
      style={{ background: 'var(--c-scrim)' }}
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="ld-pop-in w-[520px] overflow-hidden rounded-xl border border-bdhi bg-elev"
        style={{ boxShadow: 'var(--shadow-panel)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2.5 border-b border-bd px-[15px] py-[13px]">
          <span className="text-t3">
            <Icon name="search" />
          </span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command or port number…"
            className="flex-1 bg-transparent text-[13.5px] text-t1 outline-none placeholder:text-t3"
          />
          <Kbd>Esc</Kbd>
        </div>

        <div className="p-1.5">
          {filtered.length === 0 ? (
            <p className="px-2.5 py-6 text-center text-[12.5px] text-t3">No matching commands</p>
          ) : (
            filtered.map((c, i) => (
              <button
                key={c.id}
                type="button"
                onMouseEnter={() => setCursor(i)}
                onClick={c.run}
                className={`flex h-9 w-full items-center gap-[11px] rounded-[7px] px-2.5 text-left ${
                  i === cursor ? 'bg-sel' : ''
                }`}
              >
                <span className={i === cursor ? 'text-ac' : 'text-t3'}>
                  <Icon name="chevron" size={13} />
                </span>
                <span className="flex-1 text-[12.5px]">{c.label}</span>
                <span className="font-mono text-[11px] text-t3">{c.hint}</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
