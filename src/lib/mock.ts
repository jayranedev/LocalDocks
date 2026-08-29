import type {
  PortRow,
  ProcessDetail,
  ProcessRow,
  Protocol,
  Relevance,
  Service,
  Snapshot,
  SystemTelemetry,
  TerminateRequest,
  TerminateResult,
} from '../types';
import { makeProcessId } from '../types';

/**
 * Mock backend.
 *
 * Stands in for the Rust sampler until it exists. It deliberately mimics the
 * real thing's *shape*: it owns its own cadence, it holds previous samples so
 * CPU can drift rather than jump, and it emits whole snapshots. When the Rust
 * side lands, this file is the only thing that gets deleted.
 */

const TCP: Protocol = 'TCP';

interface Seed {
  id: string;
  label: string;
  framework: string | null;
  processName: string;
  pid: number;
  parentPid: number;
  cpu: number;
  memMb: number;
  threads: number;
  startedAt: string;
  ports: Array<{ port: number; address: string }>;
  /**
   * What the mock claims the Developer Registry concluded.
   *
   * The real classification happens in Rust against a live command line; this
   * is a fixed stand-in so the mock exercises all three outcomes. Two of the
   * seeds are deliberately *not* development work, because a mock in which
   * Developer and System mode look identical would hide the bug this whole
   * layer exists to prevent.
   */
  relevance: Relevance;
  relevanceReason: string;
  /** Simulates a process owned by another account. */
  denied?: boolean;
  executable?: string;
  commandLine?: string;
  workingDirectory?: string;
}

const bootedAt = Date.now();
const ago = (seconds: number) => new Date(bootedAt - seconds * 1000).toISOString();

const SEEDS: Seed[] = [
  {
    id: 'fe',
    label: 'Frontend',
    framework: 'Vite · React',
    processName: 'node.exe',
    pid: 8420,
    parentPid: 6104,
    cpu: 2.3,
    memMb: 142,
    threads: 18,
    startedAt: ago(4342),
    relevance: 'developer',
    relevanceReason: 'Node.js launched with the Vite signature.',
    ports: [
      { port: 5173, address: '127.0.0.1' },
      { port: 5173, address: '[::1]' },
    ],
    executable: 'C:\\Program Files\\nodejs\\node.exe',
    commandLine: 'node C:\\dev\\shopfront\\node_modules\\vite\\bin\\vite.js --host',
    workingDirectory: 'C:\\dev\\shopfront',
  },
  {
    id: 'api',
    label: 'API',
    framework: 'Uvicorn · FastAPI',
    processName: 'python.exe',
    pid: 7120,
    parentPid: 6104,
    cpu: 0.9,
    memMb: 318,
    threads: 11,
    startedAt: ago(4180),
    relevance: 'developer',
    relevanceReason: 'Python launched with the Uvicorn signature.',
    ports: [{ port: 8000, address: '127.0.0.1' }],
    executable: 'C:\\Users\\jay\\.venvs\\shopfront\\Scripts\\python.exe',
    commandLine: 'python -m uvicorn app.main:app --reload --port 8000',
    workingDirectory: 'C:\\dev\\shopfront\\server',
  },
  {
    id: 'wk',
    label: 'Worker',
    framework: 'Celery',
    processName: 'python.exe',
    pid: 7344,
    parentPid: 6104,
    cpu: 0.2,
    memMb: 126,
    threads: 7,
    startedAt: ago(4166),
    relevance: 'developer',
    relevanceReason: 'Python launched with the Celery signature.',
    ports: [{ port: 8001, address: '127.0.0.1' }],
    executable: 'C:\\Users\\jay\\.venvs\\shopfront\\Scripts\\python.exe',
    commandLine: 'celery -A app.worker worker --loglevel=info',
    workingDirectory: 'C:\\dev\\shopfront\\server',
  },
  {
    id: 'sb',
    label: 'Storybook',
    framework: 'Storybook',
    processName: 'node.exe',
    pid: 9088,
    parentPid: 6104,
    cpu: 0.4,
    memMb: 210,
    threads: 14,
    startedAt: ago(1560),
    relevance: 'developer',
    relevanceReason: 'Node.js launched with the Storybook signature.',
    ports: [
      { port: 6006, address: '127.0.0.1' },
      { port: 6006, address: '[::1]' },
    ],
    executable: 'C:\\Program Files\\nodejs\\node.exe',
    commandLine: 'node C:\\dev\\shopfront\\node_modules\\.bin\\storybook dev -p 6006',
    workingDirectory: 'C:\\dev\\shopfront',
  },
  {
    id: 'pg',
    label: 'PostgreSQL',
    framework: 'postgres 16',
    processName: 'postgres.exe',
    pid: 6312,
    parentPid: 1284,
    cpu: 0.1,
    memMb: 421,
    threads: 9,
    startedAt: ago(273_600),
    relevance: 'developer',
    relevanceReason: 'postgres.exe launched with a registered development signature.',
    ports: [{ port: 5432, address: '127.0.0.1' }],
    denied: true,
  },
  {
    id: 'rd',
    label: 'Redis',
    framework: 'redis 7',
    processName: 'redis-server.exe',
    pid: 6488,
    parentPid: 1284,
    cpu: 0.0,
    memMb: 38,
    threads: 5,
    startedAt: ago(273_600),
    relevance: 'developer',
    relevanceReason: 'redis-server.exe launched with a registered development signature.',
    ports: [{ port: 6379, address: '127.0.0.1' }],
    executable: 'C:\\Program Files\\Redis\\redis-server.exe',
    commandLine: 'redis-server.exe --port 6379',
    workingDirectory: 'C:\\Program Files\\Redis',
  },
  {
    id: 'spotify',
    label: 'Spotify',
    framework: null,
    processName: 'Spotify.exe',
    pid: 12640,
    parentPid: 1,
    cpu: 0.6,
    memMb: 412,
    threads: 52,
    startedAt: ago(7400),
    relevance: 'system',
    relevanceReason: 'Spotify is a media application.',
    ports: [{ port: 57621, address: '0.0.0.0' }],
    executable: 'C:\\Program Files\\WindowsApps\\Spotify.exe',
    commandLine: 'Spotify.exe',
  },
  {
    id: 'editor',
    label: 'Code',
    framework: null,
    processName: 'Code.exe',
    pid: 4820,
    parentPid: 1,
    cpu: 4.1,
    memMb: 886,
    threads: 64,
    startedAt: ago(9200),
    relevance: 'unknown',
    relevanceReason:
      'code is in neither the developer nor the system registry, so it is not classified.',
    ports: [{ port: 23674, address: '127.0.0.1' }],
    executable: 'C:\\Users\\jay\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe',
    commandLine: 'Code.exe --type=utility --utility-sub-type=node.mojom.NodeService',
  },
];

const EXTRA_PROCESSES = [
  { pid: 6104, parentPid: 4820, name: 'pwsh.exe', cpu: 0.0, memMb: 62, threads: 6, startedAt: ago(8600) },
  { pid: 9312, parentPid: 8420, name: 'esbuild.exe', cpu: 0.0, memMb: 44, threads: 4, startedAt: ago(4300) },
];

/** Terminated PIDs, so the mock kill actually removes a row. */
const terminated = new Set<number>();
let sequence = 0;

/** Deterministic-ish jitter so numbers move without flickering wildly. */
function drift(base: number, tick: number, spread: number): number {
  const wave = Math.sin(tick / 3 + base) + Math.sin(tick / 7 + base * 2) * 0.5;
  return Math.max(0, base + wave * spread);
}

export function buildSnapshot(): Snapshot {
  sequence += 1;
  const now = Date.now();
  const live = SEEDS.filter((s) => !terminated.has(s.pid));

  const services: Service[] = live.map((s) => ({
    id: makeProcessId(s.pid, s.startedAt),
    label: s.label,
    framework: s.framework,
    processName: s.processName,
    pid: s.pid,
    parentPid: s.parentPid,
    cpuPercent: Number(drift(s.cpu, sequence, s.cpu > 1 ? 0.6 : 0.12).toFixed(1)),
    memoryBytes: Math.round(drift(s.memMb, sequence, 1.5) * 1024 * 1024),
    threadCount: s.threads,
    startedAt: s.startedAt,
    uptimeSeconds: (now - new Date(s.startedAt).getTime()) / 1000,
    endpoints: s.ports.map((p) => ({ protocol: TCP, address: p.address, port: p.port })),
    status: 'running',
    relevance: s.relevance,
    relevanceReason: s.relevanceReason,
  }));

  const processes: ProcessRow[] = [
    ...live.map((s) => ({
      id: makeProcessId(s.pid, s.startedAt),
      pid: s.pid,
      parentPid: s.parentPid,
      name: s.processName,
      cpuPercent: Number(drift(s.cpu, sequence, 0.4).toFixed(1)),
      memoryBytes: Math.round(drift(s.memMb, sequence, 1.5) * 1024 * 1024),
      threadCount: s.threads,
      startedAt: s.startedAt,
      uptimeSeconds: (now - new Date(s.startedAt).getTime()) / 1000,
      status: 'running' as const,
      isService: true,
    })),
    ...EXTRA_PROCESSES.map((p) => ({
      id: makeProcessId(p.pid, p.startedAt),
      pid: p.pid,
      parentPid: p.parentPid,
      name: p.name,
      cpuPercent: Number(drift(p.cpu, sequence, 0.5).toFixed(1)),
      memoryBytes: Math.round(drift(p.memMb, sequence, 4) * 1024 * 1024),
      threadCount: p.threads,
      startedAt: p.startedAt,
      uptimeSeconds: (now - new Date(p.startedAt).getTime()) / 1000,
      status: p.name === 'esbuild.exe' ? ('sleeping' as const) : ('running' as const),
      isService: false,
    })),
  ];

  const ports: PortRow[] = live
    .flatMap((s) =>
      s.ports.map((p) => ({
        port: p.port,
        protocol: TCP,
        address: p.address,
        pid: s.pid,
        processId: makeProcessId(s.pid, s.startedAt),
        processName: s.processName,
        serviceLabel: s.label,
        state: 'LISTENING' as const,
      })),
    )
    .sort((a, b) => a.port - b.port || a.address.localeCompare(b.address));

  return {
    sequence,
    capturedAt: new Date().toISOString(),
    services,
    processes,
    ports,
    // Conflict detection is a backend concern and is not implemented (§B, V2).
    // null, not 0 — the UI must not claim "no conflicts" on no evidence.
    conflicts: null,
    system: telemetry(),
    timing: { totalMillis: 21.4, processesMillis: 18.1, portsMillis: 1.6, telemetryMillis: 1.7 },
    registryVersion: 1,
  };
}

/**
 * Machine-wide load, drifting the way the real readings do.
 *
 */
function telemetry(): SystemTelemetry {
  const cores = 8;
  const perCorePercent = Array.from({ length: cores }, (_, i) =>
    Number(Math.min(100, drift(12 + i * 3, sequence + i * 5, 9)).toFixed(1)),
  );
  const memoryTotalBytes = 16 * 1024 ** 3;
  const memoryUsedBytes = Math.round(drift(10.4, sequence, 0.3) * 1024 ** 3);
  const receive = Math.round(drift(240_000, sequence, 90_000));
  const read = Math.round(drift(1_400_000, sequence + 1, 900_000));
  const write = Math.round(drift(600_000, sequence + 4, 400_000));
  const transmit = Math.round(drift(40_000, sequence + 3, 18_000));

  return {
    cpuPercent: Number(
      (perCorePercent.reduce((a, b) => a + b, 0) / perCorePercent.length).toFixed(1),
    ),
    perCorePercent,
    logicalProcessors: cores,
    memoryTotalBytes,
    memoryUsedBytes,
    memoryPercent: Number(((memoryUsedBytes / memoryTotalBytes) * 100).toFixed(1)),
    network: {
      receiveBytesPerSec: receive,
      transmitBytesPerSec: transmit,
      interfaces: [
        {
          name: 'Ethernet',
          description: 'Mock Gigabit Adapter',
          receiveBytesPerSec: receive,
          transmitBytesPerSec: transmit,
          linkSpeedBitsPerSec: 1_000_000_000,
        },
      ],
    },
    storage: {
      readBytesPerSec: read,
      writeBytesPerSec: write,
      activePercent: Number(Math.min(100, drift(6, sequence + 2, 5)).toFixed(1)),
      drives: [
        {
          number: 0,
          model: 'PhysicalDrive0',
          readBytesPerSec: read,
          writeBytesPerSec: write,
          activePercent: Number(Math.min(100, drift(6, sequence + 2, 5)).toFixed(1)),
        },
      ],
    },
  };
}

export function buildDetail(processId: string): ProcessDetail {
  const seed = SEEDS.find((s) => makeProcessId(s.pid, s.startedAt) === processId);

  if (!seed) {
    // A non-service process (Code.exe, pwsh.exe…). Real Windows would return
    // its paths; the mock has none, so it reports unavailable rather than
    // inventing a path that would look real in the UI.
    return {
      processId,
      executable: { kind: 'unavailable' },
      commandLine: { kind: 'unavailable' },
      workingDirectory: { kind: 'unavailable' },
    };
  }
  if (seed.denied) {
    return {
      processId,
      executable: { kind: 'denied' },
      commandLine: { kind: 'denied' },
      workingDirectory: { kind: 'denied' },
    };
  }
  return {
    processId,
    executable: { kind: 'ok', value: seed.executable! },
    commandLine: { kind: 'ok', value: seed.commandLine! },
    workingDirectory: { kind: 'ok', value: seed.workingDirectory! },
  };
}

/** Mirrors the real command: verify creation time, then terminate. */
export function mockTerminate(req: TerminateRequest): TerminateResult {
  const seed = SEEDS.find((s) => s.pid === req.pid);
  if (!seed) return { kind: 'stale', message: `PID ${req.pid} no longer exists.` };
  if (seed.startedAt !== req.startedAt) {
    return {
      kind: 'stale',
      message: `PID ${req.pid} has been reused since this view was rendered. Refused.`,
    };
  }
  if (seed.denied) return { kind: 'denied' };
  terminated.add(req.pid);
  return { kind: 'terminated' };
}

