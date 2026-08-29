import { describe, expect, it } from 'vitest';
import type { PortRow, ProcessRow, Relevance, Service, Snapshot, SystemTelemetry } from '../types';
import { MODE_HINTS, MODE_LABELS, RELEVANCE_LABELS, isDeveloperService, viewSnapshot } from './view';

/**
 * Mode is presentation only, and these tests exist to keep it that way.
 *
 * Two properties matter above all, and most of what follows is one of them:
 *
 *   * **Developer mode is one coherent subgraph.** Services, processes and
 *     ports all derive from a single decision, so no screen can show half a
 *     relationship.
 *   * **This module decides nothing.** It reads `service.relevance` and
 *     nothing else — no executable names, no ports, no addresses, no ancestry.
 */

const T = '2026-08-28T09:00:00.000Z';
const id = (pid: number) => `${pid}-${T}`;

const NO_TELEMETRY: SystemTelemetry = {
  cpuPercent: null,
  perCorePercent: null,
  logicalProcessors: 8,
  memoryTotalBytes: null,
  memoryUsedBytes: null,
  memoryPercent: null,
  network: null,
  storage: null,
  gpus: null,
  thermal: null,
};

const NO_TIMING = {
  totalMillis: 0,
  processesMillis: 0,
  portsMillis: 0,
  telemetryMillis: 0,
};

function process(pid: number, parentPid: number, name: string, isService = false): ProcessRow {
  return {
    id: id(pid),
    pid,
    parentPid,
    name,
    cpuPercent: 0,
    memoryBytes: 1024,
    threadCount: 1,
    startedAt: T,
    uptimeSeconds: 1,
    status: 'running',
    isService,
  };
}

function service(
  pid: number,
  parentPid: number,
  name: string,
  port: number,
  relevance: Relevance,
): Service {
  return {
    id: id(pid),
    label: `${name}:${port}`,
    framework: null,
    processName: name,
    pid,
    parentPid,
    cpuPercent: 0,
    memoryBytes: 1024,
    threadCount: 1,
    startedAt: T,
    uptimeSeconds: 1,
    endpoints: [{ protocol: 'TCP', address: '127.0.0.1', port }],
    status: 'running',
    relevance,
    relevanceReason: `${name} classified as ${relevance} for testing.`,
  };
}

function port(
  pid: number,
  p: number,
  address = '127.0.0.1',
  processId: string | null = id(pid),
): PortRow {
  return {
    port: p,
    protocol: 'TCP',
    address,
    pid,
    processId,
    processName: 'x.exe',
    serviceLabel: null,
    state: 'LISTENING',
  };
}

/**
 * A realistic machine: one dev server, and the noise that surrounds it.
 *
 * The shape is taken from an actual development workstation, where a single
 * Vite server sat among Chrome, Spotify, iCloud, Steam, the NVIDIA helpers and
 * eight VS Code sockets — all of which are Services by the observable
 * definition, and none of which is development work.
 */
function snapshot(): Snapshot {
  return {
    sequence: 7,
    capturedAt: T,
    services: [
      service(200, 100, 'node.exe', 5173, 'developer'),
      service(400, 1, 'Spotify.exe', 57621, 'system'),
      service(500, 1, 'Code.exe', 23674, 'unknown'),
    ],
    processes: [
      process(100, 1, 'pwsh.exe'), // the shell that launched the server
      process(200, 100, 'node.exe', true), // the dev server
      process(300, 200, 'esbuild.exe'), // a worker it spawned
      process(400, 1, 'Spotify.exe', true), // a Service, but not development
      process(500, 1, 'Code.exe', true), // a Service, unclassified
      process(600, 1, 'svchost.exe'), // system infrastructure
    ],
    ports: [
      port(200, 5173),
      port(400, 57621),
      port(500, 23674),
      port(600, 135, '0.0.0.0', null), // unattributable system socket
    ],
    conflicts: null,
    system: NO_TELEMETRY,
    timing: NO_TIMING,
    registryVersion: 1,
  };
}

describe('system mode', () => {
  it('passes the snapshot through completely untouched', () => {
    const s = snapshot();
    const view = viewSnapshot(s, 'system');

    expect(view.snapshot).toBe(s);
    expect(view.snapshot.services).toHaveLength(3);
    expect(view.snapshot.processes).toHaveLength(6);
    expect(view.snapshot.ports).toHaveLength(4);
    expect(view.hidden).toEqual({ services: 0, processes: 0, ports: 0 });
  });

  it('keeps system infrastructure and unattributable sockets visible', () => {
    const view = viewSnapshot(snapshot(), 'system');
    expect(view.snapshot.processes.some((p) => p.name === 'svchost.exe')).toBe(true);
    expect(view.snapshot.ports.some((p) => p.processId === null)).toBe(true);
  });
});

describe('developer mode narrows all three lists', () => {
  it('shows only services classified developer', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    expect(view.snapshot.services.map((s) => s.processName)).toEqual(['node.exe']);
    expect(view.hidden.services).toBe(2);
  });

  it('hides a Service that is not development work', () => {
    // The correction this rule exists for: Spotify holds a listening socket on
    // a non-system port and is therefore a Service by the observable
    // definition. It is not development work, and must not appear.
    const view = viewSnapshot(snapshot(), 'developer');
    const shown = view.snapshot.services.map((s) => s.processName);
    expect(shown).not.toContain('Spotify.exe');
    expect(shown).not.toContain('Code.exe');
  });

  it('shows only the processes that own a developer service', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    expect(view.snapshot.processes.map((p) => p.pid)).toEqual([200]);
    expect(view.hidden.processes).toBe(5);
  });

  it('shows only the sockets a developer service holds', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    expect(view.snapshot.ports.map((p) => p.port)).toEqual([5173]);
    expect(view.hidden.ports).toBe(3);
  });
});

describe('the subgraph is coherent', () => {
  it('never shows a service whose process is hidden', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    const shownProcessIds = new Set(view.snapshot.processes.map((p) => p.id));
    for (const s of view.snapshot.services) {
      expect(shownProcessIds.has(s.id)).toBe(true);
    }
  });

  it('never shows a port whose owning service is hidden', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    const shownServiceIds = new Set(view.snapshot.services.map((s) => s.id));
    for (const row of view.snapshot.ports) {
      expect(row.processId).not.toBeNull();
      expect(shownServiceIds.has(row.processId as string)).toBe(true);
    }
  });

  it('holds the invariant for any mix of classifications', () => {
    const mixes: Relevance[][] = [
      ['developer', 'developer', 'developer'],
      ['system', 'system', 'system'],
      ['unknown', 'unknown', 'unknown'],
      ['developer', 'system', 'unknown'],
      ['unknown', 'developer', 'system'],
    ];

    for (const mix of mixes) {
      const s = snapshot();
      s.services = s.services.map((svc, i) => ({ ...svc, relevance: mix[i] }));
      const view = viewSnapshot(s, 'developer');

      const serviceIds = new Set(view.snapshot.services.map((x) => x.id));
      const processIds = new Set(view.snapshot.processes.map((x) => x.id));

      expect(serviceIds.size).toBe(mix.filter((r) => r === 'developer').length);
      // Every service has its process, and every process has its service.
      expect([...serviceIds].every((x) => processIds.has(x))).toBe(true);
      expect([...processIds].every((x) => serviceIds.has(x))).toBe(true);
      // Every port belongs to a shown service.
      expect(view.snapshot.ports.every((p) => serviceIds.has(p.processId as string))).toBe(true);
    }
  });
});

describe('what developer mode must never do', () => {
  it('never infers relevance from process ancestry', () => {
    // The rejected rule. `pwsh.exe` is the parent of the dev server and
    // `esbuild.exe` is its child; neither is a classified developer service,
    // so neither appears.
    const view = viewSnapshot(snapshot(), 'developer');
    const names = view.snapshot.processes.map((p) => p.name);
    expect(names).not.toContain('pwsh.exe');
    expect(names).not.toContain('esbuild.exe');
  });

  it('is unchanged by rearranging the process tree', () => {
    // Same services, completely different parentage. If ancestry were an
    // input, this would produce a different view.
    const a = snapshot();
    const b = snapshot();
    b.processes = b.processes.map((p) => ({ ...p, parentPid: 1 }));
    b.services = b.services.map((s) => ({ ...s, parentPid: 1 }));

    const va = viewSnapshot(a, 'developer');
    const vb = viewSnapshot(b, 'developer');
    expect(vb.snapshot.processes.map((p) => p.pid)).toEqual(va.snapshot.processes.map((p) => p.pid));
    expect(vb.snapshot.ports.map((p) => p.port)).toEqual(va.snapshot.ports.map((p) => p.port));
  });

  it('never uses the port number', () => {
    // The same service on four ports produces the same view. A "3000–9000 is
    // developer" rule would fail this.
    const shapes = [3000, 5173, 8080, 61123].map((p) => {
      const s = snapshot();
      s.services = [service(200, 100, 'node.exe', p, 'developer')];
      s.processes = [process(200, 100, 'node.exe', true)];
      s.ports = [port(200, p)];
      const v = viewSnapshot(s, 'developer');
      return { services: v.snapshot.services.length, ports: v.snapshot.ports.length };
    });
    expect(new Set(shapes.map((x) => JSON.stringify(x))).size).toBe(1);
  });

  it('never uses the executable name', () => {
    // The module must not know what node, python or bun are. Identical
    // classifications with different names produce identical views.
    const shapes = ['node.exe', 'python.exe', 'bun.exe', 'zig.exe', 'a.exe'].map((name) => {
      const s = snapshot();
      s.services = [service(200, 100, name, 5173, 'developer')];
      s.processes = [process(200, 100, name, true)];
      s.ports = [port(200, 5173)];
      const v = viewSnapshot(s, 'developer');
      return { services: v.snapshot.services.length, processes: v.snapshot.processes.length };
    });
    expect(new Set(shapes.map((x) => JSON.stringify(x))).size).toBe(1);
  });

  it('never uses the address a service binds', () => {
    // 0.0.0.0 is a normal, and more exposed, development binding. Filtering by
    // address would hide exactly the sockets worth noticing.
    const s = snapshot();
    s.services = [service(200, 100, 'api.exe', 8000, 'developer')];
    s.services[0].endpoints = [{ protocol: 'TCP', address: '0.0.0.0', port: 8000 }];
    s.processes = [process(200, 100, 'api.exe', true)];
    s.ports = [port(200, 8000, '0.0.0.0'), port(200, 8000, '192.168.1.5')];

    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.ports).toHaveLength(2);
    expect(view.snapshot.ports.map((p) => p.address)).toEqual(['0.0.0.0', '192.168.1.5']);
  });

  it('matches on identity rather than PID, so a recycled PID pulls nothing in', () => {
    const s = snapshot();
    // A different process that happens to hold PID 200 with a later start.
    s.processes.push({ ...process(200, 1, 'stale.exe'), id: `200-${'2026-08-28T10:00:00.000Z'}` });
    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.processes.map((p) => p.name)).toEqual(['node.exe']);
  });
});

describe('degenerate snapshots', () => {
  it('shows nothing, and is not an error, when no service is classified developer', () => {
    const s = snapshot();
    s.services = s.services.map((svc) => ({ ...svc, relevance: 'unknown' as const }));
    const view = viewSnapshot(s, 'developer');

    expect(view.snapshot.services).toHaveLength(0);
    expect(view.snapshot.processes).toHaveLength(0);
    expect(view.snapshot.ports).toHaveLength(0);
    expect(view.hidden).toEqual({ services: 3, processes: 6, ports: 4 });
  });

  it('handles an empty snapshot', () => {
    const s: Snapshot = {
      sequence: 1,
      capturedAt: T,
      services: [],
      processes: [],
      ports: [],
      conflicts: null,
      system: NO_TELEMETRY,
      timing: NO_TIMING,
      registryVersion: 1,
    };
    const view = viewSnapshot(s, 'developer');
    expect(view.total).toEqual({ services: 0, processes: 0, ports: 0 });
    expect(view.hidden).toEqual({ services: 0, processes: 0, ports: 0 });
  });
});

describe('bookkeeping', () => {
  it('carries measurement through unchanged', () => {
    const s = snapshot();
    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.sequence).toBe(s.sequence);
    expect(view.snapshot.capturedAt).toBe(s.capturedAt);
    expect(view.snapshot.conflicts).toBeNull();
    expect(view.snapshot.system).toBe(s.system);
    expect(view.snapshot.timing).toBe(s.timing);
    expect(view.snapshot.registryVersion).toBe(s.registryVersion);
  });

  it('reports totals so the UI can say how much is hidden', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    expect(view.total).toEqual({ services: 3, processes: 6, ports: 4 });
    for (const key of ['services', 'processes', 'ports'] as const) {
      expect(view.hidden[key] + view.snapshot[key].length).toBe(view.total[key]);
    }
  });

  it('is a pure function of the snapshot and the mode', () => {
    const s = snapshot();
    const before = JSON.stringify(s);
    viewSnapshot(s, 'developer');
    viewSnapshot(s, 'system');
    expect(JSON.stringify(s)).toBe(before);
  });

  it('gives every screen the same answer, because there is only one answer', () => {
    const s = snapshot();
    const a = viewSnapshot(s, 'developer');
    const b = viewSnapshot(s, 'developer');
    expect(a.snapshot.services).toEqual(b.snapshot.services);
    expect(a.snapshot.processes).toEqual(b.snapshot.processes);
    expect(a.snapshot.ports).toEqual(b.snapshot.ports);
  });

  it('labels and describes both modes and all three classifications', () => {
    expect(MODE_LABELS.developer).toBe('Developer');
    expect(MODE_LABELS.system).toBe('System');
    expect(MODE_HINTS.developer).toBeTruthy();
    expect(MODE_HINTS.system).toBeTruthy();
    expect(RELEVANCE_LABELS.developer).toBe('Developer');
    expect(RELEVANCE_LABELS.system).toBe('System');
    expect(RELEVANCE_LABELS.unknown).toBe('Unclassified');
  });

  it('exposes the one predicate the rule is built on', () => {
    expect(isDeveloperService(service(1, 1, 'a', 1, 'developer'))).toBe(true);
    expect(isDeveloperService(service(1, 1, 'a', 1, 'system'))).toBe(false);
    expect(isDeveloperService(service(1, 1, 'a', 1, 'unknown'))).toBe(false);
  });
});
