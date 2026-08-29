import type { AppMode, PortRow, Service, Snapshot } from '../types';

/**
 * The one place Developer/System mode is decided.
 *
 * Mode is presentation, never collection. The backend always produces a
 * complete snapshot — every process the user owns, every socket it can see,
 * every service classified — and this narrows what the screens are shown.
 * Nothing here can change what the sampler scans, which is why switching modes
 * costs nothing and can never make the app miss something.
 *
 * It returns a `Snapshot` of the same shape rather than a bag of filtered
 * arrays, so every screen keeps consuming the contract it already understood.
 * Overview, Services, Processes and Ports all read the output of this
 * function; none of them filters by mode itself. A second implementation is
 * how the four screens start disagreeing about what a developer service is.
 *
 * # The rule
 *
 * Developer mode is **one coherent subgraph**, derived from a single decision:
 *
 *   1. Take the services the backend classified `developer`.
 *   2. Show exactly those services.
 *   3. Show exactly the processes that own them.
 *   4. Show exactly the sockets those processes hold.
 *
 * Steps 2–4 read off step 1. There is no second rule and no per-screen
 * exception, so the three screens cannot disagree: a service is never shown
 * without its process, and a port is never shown without the service that owns
 * it.
 *
 * # What this rule replaced, and why
 *
 * An earlier version treated every Service as developer-relevant and then
 * spread outward one hop through the process tree — a service's parent and its
 * children. Both halves were wrong:
 *
 *   * **Every Service is not developer work.** A Service is an *observation*:
 *     a process the user owns holding a listening socket on a non-system port.
 *     Chrome, Spotify, iCloud, Steam, the NVIDIA helpers and every VS Code
 *     window satisfy that, and on a real machine they outnumbered the actual
 *     dev servers by roughly fifteen to one. Developer mode showed all of
 *     them, which made it indistinguishable from System mode.
 *   * **Ancestry is not evidence.** One hop from a service reaches whatever
 *     else its parent started — unrelated siblings, the shell, and from a
 *     terminal or an editor, everything running under it. Being spawned by the
 *     same thing as a dev server does not make a process part of the
 *     development picture.
 *
 * Relevance is now decided once, in Rust, by the Developer Registry
 * (`src-tauri/src/logic/registry.rs`) against the executable name and command
 * line — never by ancestry, never by port number, and never here. This module
 * only reads `service.relevance`.
 *
 * Note what is still *not* consulted: the address a service binds. A server on
 * `0.0.0.0:8000` is exactly as visible as one on `127.0.0.1:8000`, because
 * hiding the wider binding would hide the one a developer most needs to
 * notice.
 */

export interface SnapshotView {
  mode: AppMode;
  /** The complete snapshot in System mode; the narrowed one in Developer. */
  snapshot: Snapshot;
  /** What the current mode is holding back, so the UI can say so out loud. */
  hidden: { services: number; processes: number; ports: number };
  /** Always the unfiltered totals, for "2 of 31" readouts. */
  total: { services: number; processes: number; ports: number };
}

/** Is this service development work, as the backend classified it? */
export function isDeveloperService(service: Service): boolean {
  return service.relevance === 'developer';
}

/** Narrow a snapshot for presentation. */
export function viewSnapshot(snapshot: Snapshot, mode: AppMode): SnapshotView {
  const total = {
    services: snapshot.services.length,
    processes: snapshot.processes.length,
    ports: snapshot.ports.length,
  };

  /* System mode is the raw view: the snapshot passes through untouched, so
     nothing the backend can see is ever hidden from the diagnostic view. */
  if (mode === 'system') {
    return { mode, snapshot, hidden: { services: 0, processes: 0, ports: 0 }, total };
  }

  /* Step 1 — the only decision. Everything below is derived from it. */
  const services = snapshot.services.filter(isDeveloperService);
  const developerIds = new Set(services.map((s) => s.id));

  /* Step 3 — the owning processes, matched on identity rather than PID so a
     recycled PID can never pull an unrelated process into the view. Nothing
     else is added: no parents, no children, no tree. */
  const processes = snapshot.processes.filter((p) => developerIds.has(p.id));

  /* Step 4 — the sockets those services hold. A row the backend could not
     attribute has no `processId`, so it can never match; such rows are system
     infrastructure by definition and stay one click away in System mode. */
  const ports = snapshot.ports.filter(
    (r: PortRow) => r.processId !== null && developerIds.has(r.processId),
  );

  return {
    mode,
    /* `sequence`, `capturedAt`, `conflicts`, `system` and `registryVersion`
       carry through untouched: mode changes what is shown, not what was
       measured. */
    snapshot: { ...snapshot, services, processes, ports },
    hidden: {
      services: total.services - services.length,
      processes: total.processes - processes.length,
      ports: total.ports - ports.length,
    },
    total,
  };
}

/** Label for the mode, used by the switch, the status bar and Overview. */
export const MODE_LABELS: Record<AppMode, string> = {
  developer: 'Developer',
  system: 'System',
};

/** One-line description, for tooltips and the Overview meta row. */
export const MODE_HINTS: Record<AppMode, string> = {
  developer: 'Classified development services, their processes and their sockets.',
  system: 'Everything LocalDocks can observe, unfiltered.',
};

/** Human label for a classification, for chips and the detail panel. */
export const RELEVANCE_LABELS: Record<Service['relevance'], string> = {
  developer: 'Developer',
  system: 'System',
  unknown: 'Unclassified',
};
