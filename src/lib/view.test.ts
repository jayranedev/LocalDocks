import { describe, expect, it } from 'vitest';
import type { PortRow, ProcessRow, Service, Snapshot } from '../types';
import { MODE_HINTS, MODE_LABELS, viewSnapshot } from './view';

/**
 * Mode is presentation only, and these tests exist to keep it that way: no
 * executable names, no addresses, and System mode always identical to the
 * snapshot the backend produced.
 */

const T = '2026-08-28T09:00:00.000Z';
const id = (pid: number) => `${pid}-${T}`;

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

function service(pid: number, parentPid: number, name: string, port: number): Service {
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
  };
}

function port(pid: number, p: number, address = '127.0.0.1', processId: string | null = id(pid)): PortRow {
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

/** A shell that started a dev server, which spawned a worker, plus noise. */
function snapshot(): Snapshot {
  return {
    sequence: 7,
    capturedAt: T,
    services: [service(200, 100, 'node.exe', 5173)],
    processes: [
      process(100, 1, 'pwsh.exe'), // the shell that launched it
      process(200, 100, 'node.exe', true), // the dev server
      process(300, 200, 'esbuild.exe'), // a worker it spawned
      process(400, 1, 'Spotify.exe'), // unrelated
      process(500, 1, 'svchost.exe'), // system infrastructure
    ],
    ports: [
      port(200, 5173),
      port(500, 135, '0.0.0.0', null), // unattributable system socket
      port(400, 57621), // unrelated app
    ],
    conflicts: null,
  };
}

describe('system mode', () => {
  it('passes the snapshot through completely untouched', () => {
    const s = snapshot();
    const view = viewSnapshot(s, 'system');

    expect(view.snapshot).toBe(s);
    expect(view.snapshot.processes).toHaveLength(5);
    expect(view.snapshot.ports).toHaveLength(3);
    expect(view.hidden).toEqual({ processes: 0, ports: 0 });
  });

  it('keeps system infrastructure and unattributable sockets visible', () => {
    const view = viewSnapshot(snapshot(), 'system');
    expect(view.snapshot.processes.some((p) => p.name === 'svchost.exe')).toBe(true);
    expect(view.snapshot.ports.some((p) => p.processId === null)).toBe(true);
  });
});

describe('developer mode', () => {
  it('keeps the service, its parent and its child', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    const pids = view.snapshot.processes.map((p) => p.pid).sort((a, b) => a - b);
    expect(pids).toEqual([100, 200, 300]);
  });

  it('drops unrelated applications and system infrastructure', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    const names = view.snapshot.processes.map((p) => p.name);
    expect(names).not.toContain('Spotify.exe');
    expect(names).not.toContain('svchost.exe');
    expect(view.hidden.processes).toBe(2);
  });

  it('shows a socket exactly when its owning process is shown', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    const visiblePids = new Set(view.snapshot.processes.map((p) => p.pid));
    for (const row of view.snapshot.ports) {
      expect(visiblePids.has(row.pid)).toBe(true);
    }
    expect(view.snapshot.ports.map((p) => p.port)).toEqual([5173]);
    expect(view.hidden.ports).toBe(2);
  });

  it('never narrows the services themselves', () => {
    const s = snapshot();
    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.services).toEqual(s.services);
  });

  it('carries sequence, capturedAt and conflicts through unchanged', () => {
    const s = snapshot();
    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.sequence).toBe(s.sequence);
    expect(view.snapshot.capturedAt).toBe(s.capturedAt);
    expect(view.snapshot.conflicts).toBeNull();
  });

  it('reports totals so the UI can say how much is hidden', () => {
    const view = viewSnapshot(snapshot(), 'developer');
    expect(view.total).toEqual({ processes: 5, ports: 3 });
    expect(view.hidden.processes + view.snapshot.processes.length).toBe(view.total.processes);
    expect(view.hidden.ports + view.snapshot.ports.length).toBe(view.total.ports);
  });
});

describe('what developer mode must never become', () => {
  it('keeps a wildcard binding eligible', () => {
    // 0.0.0.0 is a normal, and more exposed, development binding. Filtering by
    // address would hide exactly the sockets worth noticing.
    const s = snapshot();
    s.services = [service(200, 100, 'api.exe', 8000)];
    s.services[0].endpoints = [{ protocol: 'TCP', address: '0.0.0.0', port: 8000 }];
    s.ports = [port(200, 8000, '0.0.0.0')];

    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.ports).toHaveLength(1);
    expect(view.snapshot.ports[0].address).toBe('0.0.0.0');
    expect(view.snapshot.processes.some((p) => p.pid === 200)).toBe(true);
  });

  it('treats every runtime the same, whatever it is called', () => {
    // The rule must not know what node, python or bun are. Two processes with
    // identical relationships and different names must be treated identically.
    const build = (name: string): Snapshot => ({
      sequence: 1,
      capturedAt: T,
      services: [service(200, 100, name, 5173)],
      processes: [process(200, 100, name, true), process(999, 1, 'other.exe')],
      ports: [port(200, 5173)],
      conflicts: null,
    });

    const shapes = ['node.exe', 'python.exe', 'bun.exe', 'deno.exe', 'zig.exe', 'a.exe'].map(
      (name) => {
        const v = viewSnapshot(build(name), 'developer');
        return { processes: v.snapshot.processes.length, ports: v.snapshot.ports.length };
      },
    );

    expect(new Set(shapes.map((s) => JSON.stringify(s))).size).toBe(1);
  });

  it('keeps a service whose only socket is on a public interface', () => {
    const s = snapshot();
    s.ports = [port(200, 5173, '192.168.1.5')];
    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.ports).toHaveLength(1);
  });

  it('shows nothing but is not an error when no service is running', () => {
    const s = snapshot();
    s.services = [];
    s.processes = s.processes.map((p) => ({ ...p, isService: false }));

    const view = viewSnapshot(s, 'developer');
    expect(view.snapshot.processes).toHaveLength(0);
    expect(view.snapshot.ports).toHaveLength(0);
    expect(view.hidden.processes).toBe(5);
  });
});

describe('consistency across screens', () => {
  it('gives every screen the same answer, because there is only one answer', () => {
    // Overview, Services, Processes and Ports all read this one output. If any
    // of them filtered by mode itself, this is the test that would stop making
    // sense.
    const s = snapshot();
    const a = viewSnapshot(s, 'developer');
    const b = viewSnapshot(s, 'developer');

    expect(a.snapshot.processes).toEqual(b.snapshot.processes);
    expect(a.snapshot.ports).toEqual(b.snapshot.ports);
    expect(a.snapshot.services).toEqual(b.snapshot.services);
  });

  it('is a pure function of the snapshot and the mode', () => {
    const s = snapshot();
    const before = JSON.stringify(s);
    viewSnapshot(s, 'developer');
    viewSnapshot(s, 'system');
    expect(JSON.stringify(s)).toBe(before);
  });

  it('labels and describes both modes', () => {
    expect(MODE_LABELS.developer).toBe('Developer');
    expect(MODE_LABELS.system).toBe('System');
    expect(MODE_HINTS.developer).toBeTruthy();
    expect(MODE_HINTS.system).toBeTruthy();
  });
});
