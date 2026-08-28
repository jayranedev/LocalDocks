import { describe, expect, it } from 'vitest';
import { actionablePort, buildDetailTarget, isLoopbackTcp } from './detail';
import { makeProcessId, type Endpoint, type Snapshot } from '../types';

const STARTED = '2026-08-28T09:00:00.000Z';
const SVC_ID = makeProcessId(8420, STARTED);
const PLAIN_ID = makeProcessId(4820, STARTED);

const tcp = (address: string, port: number): Endpoint => ({ protocol: 'TCP', address, port });

function snapshot(): Snapshot {
  return {
    sequence: 1,
    capturedAt: STARTED,
    conflicts: null,
    services: [
      {
        id: SVC_ID,
        label: 'Frontend',
        framework: 'Vite · React',
        processName: 'node.exe',
        pid: 8420,
        parentPid: 6104,
        cpuPercent: 2.3,
        memoryBytes: 142 * 1024 * 1024,
        threadCount: 18,
        startedAt: STARTED,
        uptimeSeconds: 4342,
        endpoints: [tcp('127.0.0.1', 5173), tcp('[::1]', 5173)],
        status: 'running',
      },
    ],
    processes: [
      {
        id: SVC_ID,
        pid: 8420,
        parentPid: 6104,
        name: 'node.exe',
        cpuPercent: 2.3,
        memoryBytes: 142 * 1024 * 1024,
        threadCount: 18,
        startedAt: STARTED,
        uptimeSeconds: 4342,
        status: 'running',
        isService: true,
      },
      {
        id: PLAIN_ID,
        pid: 4820,
        parentPid: 1,
        name: 'Code.exe',
        cpuPercent: 4.1,
        memoryBytes: 886 * 1024 * 1024,
        threadCount: 64,
        startedAt: STARTED,
        uptimeSeconds: 9200,
        status: 'running',
        isService: false,
      },
    ],
    ports: [
      {
        port: 5173,
        protocol: 'TCP',
        address: '127.0.0.1',
        pid: 8420,
        processId: SVC_ID,
        processName: 'node.exe',
        serviceLabel: 'Frontend',
        state: 'LISTENING',
      },
      {
        port: 5173,
        protocol: 'TCP',
        address: '[::1]',
        pid: 8420,
        processId: SVC_ID,
        processName: 'node.exe',
        serviceLabel: 'Frontend',
        state: 'LISTENING',
      },
    ],
  };
}

describe('buildDetailTarget', () => {
  it('returns null for a process that is not in the snapshot', () => {
    expect(buildDetailTarget(snapshot(), ' 9999-nope')).toBeNull();
  });

  it('prefers service identity for a process that is a service', () => {
    const t = buildDetailTarget(snapshot(), SVC_ID)!;
    expect(t.title).toBe('Frontend');
    expect(t.badge).toBe('Vite · React');
    expect(t.isService).toBe(true);
    expect(t.endpoints).toHaveLength(2);
  });

  it('falls back to the process name when it is not a service', () => {
    const t = buildDetailTarget(snapshot(), PLAIN_ID)!;
    expect(t.title).toBe('Code.exe');
    expect(t.badge).toBeNull();
    expect(t.isService).toBe(false);
    expect(t.endpoints).toEqual([]);
  });

  it('carries the identity a verified terminate needs', () => {
    const t = buildDetailTarget(snapshot(), SVC_ID)!;
    expect(t.pid).toBe(8420);
    expect(t.startedAt).toBe(STARTED);
    expect(makeProcessId(t.pid, t.startedAt)).toBe(t.processId);
  });

  it('reconstructs endpoints from the port table when there is no service entry', () => {
    const snap = snapshot();
    snap.services = []; // socket held, but did not qualify as a service
    const t = buildDetailTarget(snap, SVC_ID)!;
    expect(t.endpoints).toHaveLength(2);
    expect(t.title).toBe('node.exe');
  });

  it('passes the clicked socket through as the highlight', () => {
    const t = buildDetailTarget(snapshot(), SVC_ID, { port: 5173, address: '[::1]' })!;
    expect(t.highlight).toEqual({ port: 5173, address: '[::1]' });
  });
});

describe('isLoopbackTcp', () => {
  it('accepts loopback and wildcard TCP', () => {
    expect(isLoopbackTcp(tcp('127.0.0.1', 5173))).toBe(true);
    expect(isLoopbackTcp(tcp('[::1]', 5173))).toBe(true);
    expect(isLoopbackTcp(tcp('0.0.0.0', 5173))).toBe(true);
  });

  it('rejects an external interface', () => {
    expect(isLoopbackTcp(tcp('192.168.1.42', 5173))).toBe(false);
  });

  it('rejects UDP, which has no URL to open', () => {
    expect(isLoopbackTcp({ protocol: 'UDP', address: '127.0.0.1', port: 5173 })).toBe(false);
  });
});


describe('actionablePort', () => {
  it('is null for a process holding no sockets', () => {
    const t = buildDetailTarget(snapshot(), PLAIN_ID)!;
    expect(actionablePort(t)).toBeNull();
  });

  it('picks the lowest openable port when nothing was highlighted', () => {
    const t = buildDetailTarget(snapshot(), SVC_ID)!;
    expect(actionablePort(t)).toBe(5173);
  });

  it('honours the socket the user actually clicked', () => {
    const t = buildDetailTarget(snapshot(), SVC_ID, { port: 5173, address: '[::1]' })!;
    expect(actionablePort(t)).toBe(5173);
  });

  it('is null when the highlighted socket is not openable', () => {
    const snap = snapshot();
    snap.services[0].endpoints = [tcp('192.168.1.42', 5173)];
    const t = buildDetailTarget(snap, SVC_ID, { port: 5173, address: '192.168.1.42' })!;
    expect(actionablePort(t)).toBeNull();
  });
});
