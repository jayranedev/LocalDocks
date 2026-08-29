import type { AppMode, PortRow, ProcessRow, Snapshot } from '../types';

/**
 * The one place Developer/System mode is decided.
 *
 * Mode is presentation, never collection. The backend always produces a
 * complete snapshot — every process the user owns, every socket it can see —
 * and this narrows what the screens are shown. Nothing here can change what
 * the sampler scans, which is why switching modes costs nothing and can never
 * make the app miss something.
 *
 * It returns a `Snapshot` of the same shape rather than a bag of filtered
 * arrays, so every screen keeps consuming the contract it already understood.
 * Overview, Services, Processes and Ports all read the output of this
 * function; none of them filters by mode itself. A second implementation is
 * how the four screens start disagreeing about what a developer service is.
 */

export interface SnapshotView {
  mode: AppMode;
  /** The complete snapshot in System mode; the narrowed one in Developer. */
  snapshot: Snapshot;
  /** What the current mode is holding back, so the UI can say so out loud. */
  hidden: { processes: number; ports: number };
  /** Always the unfiltered totals, for "68 of 247" readouts. */
  total: { processes: number; ports: number };
}

/**
 * Is this process part of the development picture?
 *
 * Built only from relationships the backend already observed:
 *
 *  - it holds a listening socket on a non-system port and is therefore a
 *    Service (`isService`, decided in Rust by the service model), or
 *  - it is the child of a Service — the workers a dev server spawns, and
 *  - it is the parent of a Service — the shell or task runner that started it.
 *
 * One hop in each direction, deliberately. Walking the whole tree reaches
 * `explorer.exe` and from there everything, which would make Developer mode
 * mean nothing.
 *
 * What this rule is NOT:
 *
 *  - It is not a list of executable names. `node`, `python`, `cargo` and the
 *    rest appear nowhere; docs/ARCHITECTURE.md § 1 rejects allowlists as
 *    "permanently wrong in both directions", and a rule that had to be updated
 *    for every new runtime would be wrong the day someone tries Bun.
 *  - It is not localhost-only. Addresses are not consulted at all, so a server
 *    on `0.0.0.0:8000` is exactly as eligible as one on `127.0.0.1:8000`.
 *    Filtering by address would hide the bindings a developer most needs to
 *    notice.
 */
function isDevelopmentRelevant(
  process: ProcessRow,
  servicePids: ReadonlySet<number>,
  serviceParentPids: ReadonlySet<number>,
): boolean {
  return (
    process.isService ||
    servicePids.has(process.parentPid) ||
    serviceParentPids.has(process.pid)
  );
}

/** Narrow a snapshot for presentation. */
export function viewSnapshot(snapshot: Snapshot, mode: AppMode): SnapshotView {
  const total = { processes: snapshot.processes.length, ports: snapshot.ports.length };

  /* System mode is the raw view: the snapshot passes through untouched, so
     nothing the backend can see is ever hidden from the diagnostic view. */
  if (mode === 'system') {
    return { mode, snapshot, hidden: { processes: 0, ports: 0 }, total };
  }

  const servicePids = new Set(snapshot.services.map((s) => s.pid));
  const serviceParentPids = new Set(snapshot.services.map((s) => s.parentPid));

  const processes = snapshot.processes.filter((p) =>
    isDevelopmentRelevant(p, servicePids, serviceParentPids),
  );

  /* A socket is shown when its owning process is. Rows the backend could not
     attribute have no owning process, so they are system infrastructure by
     definition and drop out here — they remain one click away in System mode. */
  const visiblePids = new Set(processes.map((p) => p.pid));
  const ports = snapshot.ports.filter((r: PortRow) => visiblePids.has(r.pid));

  return {
    mode,
    /* Services are the definition of the development picture, so they are
       never narrowed. `sequence`, `capturedAt` and `conflicts` carry through
       untouched: mode changes what is shown, not what was measured. */
    snapshot: { ...snapshot, processes, ports },
    hidden: {
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
  developer: 'Services, the processes around them, and their sockets.',
  system: 'Everything LocalDocks can observe, unfiltered.',
};
