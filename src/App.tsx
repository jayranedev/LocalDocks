import { useCallback, useEffect, useMemo, useState } from 'react';
import type { AppMode, ProcessId, ScreenId, Theme } from './types';
import { isInteractive } from './config/flags';
import { buildDetailTarget } from './lib/detail';
import { secondsSince } from './lib/format';
import { loadSettings, saveSettings } from './lib/settings';
import { viewSnapshot } from './lib/view';
import { useSnapshot, useTheme, useTicker } from './hooks/useSnapshot';

import { TitleBar } from './components/TitleBar';
import { Sidebar } from './components/Sidebar';
import { StatusBar } from './components/StatusBar';
import { ProcessDetailPanel } from './components/ProcessDetailPanel';
import { TerminateDialog } from './components/TerminateDialog';
import { CommandPalette } from './components/CommandPalette';
import { EmptyState, ErrorState, LoadingState } from './components/states';

import { Overview } from './screens/Overview';
import { Services } from './screens/Services';
import { Processes } from './screens/Processes';
import { Ports } from './screens/Ports';
import { Projects } from './screens/Projects';
import { Docker, Logs, Wsl } from './screens/Previews';
import { Settings } from './screens/Settings';

interface Selection {
  processId: ProcessId;
  /** Set when the user arrived from the Ports table, so the panel can mark it. */
  highlight: { port: number; address: string } | null;
}

export default function App() {
  const [screen, setScreen] = useState<ScreenId>('overview');
  const [settings, setSettings] = useState(loadSettings);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [terminating, setTerminating] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);

  useTheme(settings.theme);
  const state = useSnapshot(settings.intervalMs);
  useTicker(100); // keeps the "scanned N s ago" readout moving

  const setTheme = useCallback((theme: Theme) => {
    setSettings((s) => {
      const next = { ...s, theme };
      saveSettings(next);
      return next;
    });
  }, []);

  const setIntervalMs = useCallback((intervalMs: number) => {
    setSettings((s) => {
      const next = { ...s, intervalMs };
      saveSettings(next);
      return next;
    });
  }, []);

  const setMode = useCallback((mode: AppMode) => {
    setSettings((s) => {
      const next = { ...s, mode };
      saveSettings(next);
      return next;
    });
  }, []);

  /* A failed scan keeps the last good snapshot on screen behind a warning,
     rather than blanking a working UI over one bad tick. */
  const snapshot =
    state.kind === 'ready' || state.kind === 'empty'
      ? state.snapshot
      : state.kind === 'error'
        ? state.stale
        : null;

  const scanError = state.kind === 'error' ? { message: state.message, detail: state.detail } : null;

  /* Developer/System is applied exactly once, here, and every screen below
     receives the result. No screen filters by mode itself — see lib/view.ts. */
  const view = useMemo(
    () => (snapshot ? viewSnapshot(snapshot, settings.mode) : null),
    [snapshot, settings.mode],
  );
  const visible = view?.snapshot ?? null;
  const services = useMemo(() => visible?.services ?? [], [visible]);

  /* Detail is built from the complete snapshot, not the narrowed one: a panel
     opened in Developer mode must not empty itself when the process it
     describes is one the mode happens to hide. */
  const target = useMemo(
    () =>
      snapshot && selection
        ? buildDetailTarget(snapshot, selection.processId, selection.highlight)
        : null,
    [snapshot, selection],
  );

  /* A terminated process disappears from the next snapshot, so `target`
     becomes null and every panel below stops rendering. Nothing needs to
     reconcile the stale selection — deriving from `target` is the reconcile. */

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
      if (e.key === 'Escape' && !paletteOpen && !terminating) setSelection(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [paletteOpen, terminating]);

  const navigate = useCallback((id: ScreenId) => {
    setScreen(id);
    setSelection(null);
  }, []);

  const selectProcess = useCallback(
    (processId: ProcessId, highlight: Selection['highlight'] = null) => {
      setTerminating(false);
      setSelection({ processId, highlight });
    },
    [],
  );

  /* Nav badges count what the current mode shows, so they always agree with
     the table the user lands on. */
  const counts = {
    services: services.length,
    processes: visible?.processes.length,
    ports: visible?.ports.length,
  };

  const selectedId = selection?.processId ?? null;
  const showDetail = target !== null && isInteractive(screen);

  return (
    <div className="flex h-full flex-col bg-background text-primary">
      <TitleBar
        onOpenPalette={() => setPaletteOpen(true)}
        onOpenSettings={() => navigate('settings')}
        mode={settings.mode}
        onModeChange={setMode}
      />

      <div className="flex min-h-0 flex-1">
        <Sidebar screen={screen} onNavigate={navigate} counts={counts} />

        <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
          {!snapshot && state.kind === 'loading' && <LoadingState />}
          {!snapshot && scanError && (
            <ErrorState message={scanError.message} detail={scanError.detail} />
          )}

          {visible && view && (
            <>
              {screen === 'overview' &&
                (state.kind === 'empty' ? (
                  <EmptyState />
                ) : (
                  <Overview
                    snapshot={visible}
                    view={view}
                    intervalMs={settings.intervalMs}
                    onSelect={selectProcess}
                  />
                ))}

              {screen === 'services' &&
                (state.kind === 'empty' ? (
                  <EmptyState />
                ) : (
                  <Services snapshot={visible} selectedId={selectedId} onSelect={selectProcess} />
                ))}

              {screen === 'processes' && (
                <Processes snapshot={visible} selectedId={selectedId} onSelect={selectProcess} />
              )}
              {screen === 'ports' && (
                <Ports snapshot={visible} selectedId={selectedId} onSelect={selectProcess} />
              )}
              {screen === 'projects' && <Projects />}
              {screen === 'logs' && <Logs />}
              {screen === 'docker' && <Docker />}
              {screen === 'wsl' && <Wsl />}
              {screen === 'settings' && (
                <Settings
                  theme={settings.theme}
                  onThemeChange={setTheme}
                  intervalMs={settings.intervalMs}
                  onIntervalChange={setIntervalMs}
                />
              )}
            </>
          )}

          {showDetail && target && (
            <ProcessDetailPanel
              key={target.processId}
              target={target}
              onClose={() => setSelection(null)}
              onTerminate={() => setTerminating(true)}
            />
          )}

          {terminating && target && (
            <TerminateDialog
              target={target}
              onClose={() => setTerminating(false)}
              onTerminated={() => {
                setTerminating(false);
                setSelection(null);
              }}
            />
          )}

          {paletteOpen && (
            <CommandPalette
              services={services}
              onClose={() => setPaletteOpen(false)}
              onNavigate={setScreen}
              onSelectService={(id) => selectProcess(id)}
            />
          )}
        </main>
      </div>

      <StatusBar
        mode={settings.mode}
        hidden={view?.hidden ?? { processes: 0, ports: 0 }}
        serviceCount={services.length}
        conflicts={snapshot?.conflicts ?? null}
        age={snapshot ? secondsSince(snapshot.capturedAt) : 0}
        intervalMs={settings.intervalMs}
        error={scanError}
      />
    </div>
  );
}
