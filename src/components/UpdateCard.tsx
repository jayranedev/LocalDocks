import type { Updates } from '../hooks/useUpdates';
import { Button, Card, Note, SettingRow } from './ui';

/**
 * The Updates section of Settings.
 *
 * Everything the update channel exposes to the user is here and nowhere else:
 * one toggle, one button, one line of status. That is deliberate — an update
 * mechanism that needs a screen of its own is doing too much.
 *
 * Three states worth reading the code for:
 *
 *   * **Managed by the Store.** No toggle, no button, one sentence. A packaged
 *     install cannot replace itself and must not appear to offer to.
 *   * **A failed check.** Rendered as a quiet line, not an error dialog. The
 *     app is a process monitor; GitHub being unreachable does not concern it.
 *   * **An update available.** The only place in the app with a primary
 *     button, because it is the only place the user is agreeing to something
 *     irreversible.
 */
interface Props {
  updates: Updates;
  autoCheck: boolean;
  onAutoCheckChange: (on: boolean) => void;
}

export function UpdateCard({ updates, autoCheck, onAutoCheckChange }: Props) {
  const { capability, state, check, install } = updates;

  if (capability?.managedByStore) {
    return (
      <Card className="px-4 py-3.5">
        <h2 className="mb-2 text-[12.5px] font-semibold">Updates</h2>
        <p className="text-[11.5px] leading-relaxed text-muted">
          This copy was installed from the Microsoft Store, which keeps it up to date.
          LocalDocks does not check for updates itself here.
        </p>
      </Card>
    );
  }

  const busy = state.kind === 'checking' || state.kind === 'installing';

  return (
    <Card className="px-4 py-3.5">
      <h2 className="mb-2 text-[12.5px] font-semibold">Updates</h2>

      <SettingRow
        label="Check automatically"
        hint="Once a day, and never while the app is starting. This is the only thing LocalDocks sends over the network."
      >
        <button
          type="button"
          role="switch"
          aria-checked={autoCheck}
          aria-label="Check for updates automatically"
          onClick={() => onAutoCheckChange(!autoCheck)}
          className={`relative h-[22px] w-[38px] flex-none rounded-full border transition-colors ${
            autoCheck ? 'border-accent bg-accent-soft' : 'border-border bg-surface-hover'
          }`}
        >
          <span
            className={`absolute top-[2px] size-[16px] rounded-full transition-all ${
              autoCheck ? 'left-[18px] bg-accent' : 'left-[2px] bg-muted'
            }`}
          />
        </button>
      </SettingRow>

      <SettingRow label="Version" hint={statusHint(state)}>
        <div className="flex items-center gap-2">
          {state.kind === 'available' ? (
            <Button icon="download" variant="primary" onClick={install} disabled={busy}>
              {state.kind === 'available' && busy ? 'Installing…' : `Install ${state.version}`}
            </Button>
          ) : (
            <Button icon="refresh" onClick={check} disabled={busy}>
              {state.kind === 'checking' ? 'Checking…' : 'Check now'}
            </Button>
          )}
        </div>
      </SettingRow>

      {state.kind === 'available' && (
        <div className="mt-[11px] flex flex-col gap-[9px]">
          <Note icon="download" tone="accent">
            LocalDocks {state.version} is available. Installing downloads it from GitHub, verifies
            its signature, and restarts the app. Your settings are kept.
          </Note>
          {state.notes && (
            <div className="max-h-[160px] overflow-auto rounded-[7px] border border-border bg-surface-hover px-3 py-[9px] text-[11.5px] leading-relaxed whitespace-pre-wrap text-secondary">
              {state.notes}
            </div>
          )}
        </div>
      )}

      {state.kind === 'installFailed' && (
        <div className="mt-[11px]">
          <Note icon="warn" tone="danger">
            {state.reason}
          </Note>
        </div>
      )}
    </Card>
  );
}

/**
 * The one line under "Version".
 *
 * A failed check reads as a fact about the network rather than a problem with
 * the app, because that is what it is.
 */
function statusHint(state: Updates['state']): string {
  switch (state.kind) {
    case 'idle':
      return 'Not checked yet this session.';
    case 'checking':
      return 'Asking GitHub for the latest stable release…';
    case 'installing':
      return 'Downloading and verifying. LocalDocks will restart when it is done.';
    case 'installFailed':
      return 'The installed version is unchanged.';
    case 'upToDate':
      return `You are on the latest stable release (${state.currentVersion}).`;
    case 'available':
      return `Installed ${state.currentVersion} · ${state.version} is available.`;
    case 'unsupported':
      return state.reason;
    case 'failed':
      return `${state.reason} Nothing else is affected.`;
  }
}
