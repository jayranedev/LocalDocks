import type { Theme } from '../types';
import { IS_DEV, MODULES } from '../config/flags';
import { BACKEND } from '../lib/ipc';
import { INTERVALS } from '../lib/settings';
import { Card, Chip, Note, PageHeader, SettingRow } from '../components/ui';

/**
 * Settings.
 *
 * Every control here is wired. Anything that could not be honestly implemented
 * for V1 was removed rather than left inert — density, loopback filtering,
 * system-port visibility and browser choice all depend on backend behaviour
 * that does not exist yet, and a toggle that does nothing is worse than an
 * absent one.
 *
 * "Confirm before terminating" was removed deliberately: a safety
 * confirmation you can switch off is not a safety confirmation. It is always on.
 */
interface Props {
  theme: Theme;
  onThemeChange: (t: Theme) => void;
  intervalMs: number;
  onIntervalChange: (ms: number) => void;
}

const THEMES: Theme[] = ['system', 'light', 'dark'];

export function Settings({ theme, onThemeChange, intervalMs, onIntervalChange }: Props) {
  return (
    <div className="ld-fade-in overflow-auto px-6 py-5">
      <PageHeader title="Settings" subtitle="Local only. Nothing here leaves this machine." />

      <div className="mt-5 flex max-w-[680px] flex-col gap-[11px]">
        <Card className="px-4 py-3.5">
          <h2 className="mb-2 text-[12.5px] font-semibold">Appearance</h2>
          <SettingRow label="Theme" hint="Follows the system by default. Remembered between launches.">
            <div className="flex gap-1 rounded-[7px] border border-bd bg-surfhi p-[3px]">
              {THEMES.map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => onThemeChange(t)}
                  className={`rounded-[5px] px-[11px] py-1 text-xs capitalize transition-colors ${
                    theme === t ? 'bg-elev font-medium text-t1' : 'text-t3 hover:text-t2'
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>
          </SettingRow>
        </Card>

        <Card className="px-4 py-3.5">
          <h2 className="mb-2 text-[12.5px] font-semibold">Monitoring</h2>
          <SettingRow
            label="Refresh interval"
            hint="The sampler owns this cadence. The UI never triggers a scan itself."
          >
            <div className="flex gap-1 rounded-[7px] border border-bd bg-surfhi p-[3px]">
              {INTERVALS.map((ms) => (
                <button
                  key={ms}
                  type="button"
                  onClick={() => onIntervalChange(ms)}
                  className={`rounded-[5px] px-2.5 py-1 font-mono text-[11px] transition-colors ${
                    intervalMs === ms ? 'bg-elev font-medium text-t1' : 'text-t3 hover:text-t2'
                  }`}
                >
                  {ms}
                </button>
              ))}
            </div>
          </SettingRow>
        </Card>

        {IS_DEV && (
          <Card className="border-bdhi px-4 py-3.5">
            <div className="mb-1 flex items-center gap-2">
              <h2 className="text-[12.5px] font-semibold">Build</h2>
              <Chip tone="quiet">DEV ONLY</Chip>
            </div>
            <p className="mt-1.5 mb-1 text-[11.5px] leading-relaxed text-t3">
              Read-only. Unreleased modules are compiled in but gated — edit{' '}
              <span className="font-mono">src/config/flags.ts</span> to change what ships.
            </p>
            {(['projects', 'logs', 'docker', 'wsl'] as const).map((id) => (
              <SettingRow
                key={id}
                label={`${id[0].toUpperCase()}${id.slice(1)} (${MODULES[id].milestone})`}
                hint={
                  MODULES[id].state === 'preview'
                    ? 'Nav item shown, disabled in production.'
                    : 'Hidden from the production nav entirely.'
                }
              >
                <Chip>{MODULES[id].state}</Chip>
              </SettingRow>
            ))}
            <SettingRow label="Backend" hint="Falls back to the mock sampler outside Tauri.">
              <Chip tone={BACKEND === 'tauri' ? 'accent' : 'neutral'}>{BACKEND}</Chip>
            </SettingRow>
          </Card>
        )}

        <Card className="px-4 py-3.5">
          <h2 className="mb-2 text-[12.5px] font-semibold">About</h2>
          <p className="text-xs leading-[1.8] text-t2">
            LocalDocks v0.1.0 · MIT · Silent Minds
            <br />
            <span className="font-mono text-[11px] text-t3">com.silentminds.localdocks</span>
          </p>
          <div className="mt-[11px]">
            <Note icon="lock" tone="grn">
              Running without elevation. LocalDocks never requests administrator rights.
            </Note>
          </div>
        </Card>
      </div>
    </div>
  );
}
