import type { ScreenId } from '../types';

/**
 * Feature flags.
 *
 * Every module is compiled in. What differs between builds is what the
 * navigation exposes:
 *
 *   shipped   live, fully functional
 *   preview   visible but disabled, labelled with the version it lands in
 *   hidden    absent from the production nav, reachable only in a dev build
 *
 * This is the "tiered nav" decision: a production V1 shows five working
 * modules plus one honest "coming in V2", rather than nine items where six
 * are inert.
 */
export type FlagState = 'shipped' | 'preview' | 'hidden';

export const IS_DEV = import.meta.env.DEV;

interface ModuleFlag {
  state: FlagState;
  /** Version this module is planned for. Null once shipped. */
  milestone: string | null;
}

export const MODULES: Record<ScreenId, ModuleFlag> = {
  overview: { state: 'shipped', milestone: null },
  services: { state: 'shipped', milestone: null },
  processes: { state: 'shipped', milestone: null },
  ports: { state: 'shipped', milestone: null },
  settings: { state: 'shipped', milestone: null },
  projects: { state: 'preview', milestone: 'V2' },
  logs: { state: 'hidden', milestone: 'V3' },
  docker: { state: 'hidden', milestone: 'V3' },
  wsl: { state: 'hidden', milestone: 'V3' },
};

/** Hidden modules appear in the nav only in a dev build. */
export function isVisible(id: ScreenId): boolean {
  const flag = MODULES[id];
  if (flag.state === 'hidden') return IS_DEV;
  return true;
}

/** Preview modules render their design behind a "coming soon" card. */
export function isInteractive(id: ScreenId): boolean {
  return MODULES[id].state === 'shipped';
}
