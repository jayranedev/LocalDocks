/**
 * Domain types shared with the Rust backend.
 *
 * These are the IPC contract. Keep them in sync with the `serde` structs in
 * src-tauri/src/models/ — a change here without a change there is a runtime
 * failure, not a compile error.
 */

export type Protocol = 'TCP' | 'UDP';

/** One listening socket. A service may hold several (IPv4 + IPv6 is routine). */
export interface Endpoint {
  protocol: Protocol;
  /** Presentation form: "127.0.0.1" or "[::1]". */
  address: string;
  port: number;
}

/**
 * A field the backend may legitimately fail to read.
 *
 * LocalDocks never elevates, so any process owned by another account will
 * refuse OpenProcess for its executable path, command line and working
 * directory. Encoding that in the type means the UI cannot silently render a
 * denial as an empty string — it has to handle the case.
 */
export type FieldState<T> =
  | { kind: 'ok'; value: T }
  | { kind: 'denied' }
  | { kind: 'unavailable' };

/**
 * Process identity: `${pid}-${startedAt}`.
 *
 * A bare PID is not an identity — Windows recycles them. Pairing it with the
 * process creation time is what makes a row safe to act on, and it is the key
 * every detail lookup and destructive command is addressed by.
 */
export type ProcessId = string;

export function makeProcessId(pid: number, startedAt: string): ProcessId {
  return `${pid}-${startedAt}`;
}

/**
 * What the Developer Registry made of a service.
 *
 * Three outcomes, not two. `unknown` is the default and it is a real answer:
 * the registry is not exhaustive, so a service it has never seen is reported
 * as unrecognised rather than guessed into one of the other two.
 *
 * Developer mode shows `developer` and nothing else. `system` and `unknown`
 * both hide, but they stay distinct because they are different claims — "this
 * is Spotify" versus "this has not been classified" — and only the second
 * means the registry has a gap worth filing.
 */
export type Relevance = 'developer' | 'system' | 'unknown';

/**
 * Tier 1 — cheap, refreshed on every sampler tick.
 *
 * A Service is a process holding at least one listening socket on a
 * non-system port, owned by the current user.
 */
export interface Service {
  id: ProcessId;
  /** Friendly name, derived from the endpoint or the process. */
  label: string;
  /** "Vite · React", "Uvicorn · FastAPI" — null until project detection (V2). */
  framework: string | null;
  processName: string;
  pid: number;
  parentPid: number;
  cpuPercent: number;
  memoryBytes: number;
  threadCount: number;
  /** Process creation time, ISO-8601. See ProcessId. */
  startedAt: string;
  uptimeSeconds: number;
  endpoints: Endpoint[];
  status: 'running' | 'stopped';
  /** What the Developer Registry made of this service. */
  relevance: Relevance;
  /**
   * One sentence naming the rule that produced `relevance` — a registry entry,
   * a matched command-line signature, or the absence of both. Never empty: a
   * classification the user cannot check is one they cannot correct.
   */
  relevanceReason: string;
}

/**
 * A row in the Processes view. Every process the current user owns.
 *
 * Carries the same identity as Service, because a Service *is* a process —
 * that is what lets all three screens share one detail panel and one
 * verified-terminate path.
 */
export interface ProcessRow {
  id: ProcessId;
  pid: number;
  parentPid: number;
  name: string;
  cpuPercent: number;
  memoryBytes: number;
  threadCount: number;
  startedAt: string;
  uptimeSeconds: number;
  status: 'running' | 'sleeping';
  /** True when this process also appears in Services. */
  isService: boolean;
}

/** A row in the Ports view. One row per socket, deliberately unmerged. */
export interface PortRow {
  port: number;
  protocol: Protocol;
  address: string;
  pid: number;
  /**
   * Identity of the owning process, or null when the backend could not
   * attribute the socket (another account's process, or it exited mid-scan).
   * Null means the row is informational only — no actions offered.
   */
  processId: ProcessId | null;
  processName: string;
  serviceLabel: string | null;
  state: 'LISTENING';
}

/** Tier 2 — expensive. Fetched when a detail panel opens, never in the scan loop. */
export interface ProcessDetail {
  processId: ProcessId;
  executable: FieldState<string>;
  commandLine: FieldState<string>;
  workingDirectory: FieldState<string>;
}

/**
 * Machine-wide load, sampled once per tick.
 *
 * Every reading is nullable and **null always means "not measured", never
 * "measured as zero"**. That distinction is the contract: a dashboard showing
 * 0 °C because a provider failed is worse than one showing nothing, because the
 * reader cannot tell the difference.
 *
 * It applies at two levels. Null on a whole section — `network`, `storage`,
 * `gpus`, `thermal` — means the provider is not present on this machine at all.
 * Null on a single rate inside a present section means the value could not be
 * computed this tick, almost always because it is the first sample and a rate
 * needs two.
 *
 * CPU and memory are flat because they always were. Network and storage nest
 * one level, and only because each genuinely has per-device detail the
 * machine-wide figure is derived from.
 */
export interface SystemTelemetry {
  /** Machine-wide utilisation over the last interval, 0–100. */
  cpuPercent: number | null;
  /** Per-logical-processor utilisation, in the order Windows enumerates. */
  perCorePercent: number[] | null;
  logicalProcessors: number;
  /**
   * Physical memory installed in the machine. Never to be confused with
   * `ProcessRow.memoryBytes`, which is one process's working set.
   */
  memoryTotalBytes: number | null;
  memoryUsedBytes: number | null;
  memoryPercent: number | null;
  network: NetworkTelemetry | null;
  storage: StorageTelemetry | null;
}

/**
 * Machine-wide network throughput.
 *
 * Derived from cumulative octet counters, never read as an instantaneous rate.
 * The totals are the sum of the per-interface rates that could actually be
 * computed, so an interface that appeared this tick contributes nothing rather
 * than its whole lifetime total.
 */
export interface NetworkTelemetry {
  receiveBytesPerSec: number | null;
  transmitBytesPerSec: number | null;
  /** Operational, non-loopback, non-filter interfaces only. */
  interfaces: NetworkInterface[];
}

export interface NetworkInterface {
  /** The name Windows shows, e.g. "Ethernet". */
  name: string;
  /** The adapter's own description. */
  description: string;
  receiveBytesPerSec: number | null;
  transmitBytesPerSec: number | null;
  linkSpeedBitsPerSec: number | null;
}

/** System-level disk activity. Per-process disk accounting is not V1. */
export interface StorageTelemetry {
  readBytesPerSec: number | null;
  writeBytesPerSec: number | null;
  /** The busiest drive's active time, not a sum across drives. */
  activePercent: number | null;
  drives: StorageDrive[];
}

export interface StorageDrive {
  /** The physical drive number, as in `\\.\PhysicalDrive0`. */
  number: number;
  model: string;
  readBytesPerSec: number | null;
  writeBytesPerSec: number | null;
  /** Share of the interval the drive was not idle, 0–100. */
  activePercent: number | null;
}

/** How long one sampler tick took, in milliseconds. */
export interface ScanTiming {
  totalMillis: number;
  processesMillis: number;
  portsMillis: number;
  telemetryMillis: number;
}

/** Everything one sampler tick produces. */
export interface Snapshot {
  /** Monotonic tick counter. */
  sequence: number;
  /** When the sampler completed this scan, ISO-8601. */
  capturedAt: string;
  services: Service[];
  processes: ProcessRow[];
  ports: PortRow[];
  /**
   * Ports bound by more than one PID.
   *
   * `null` means the backend does not compute this yet — the UI renders "—"
   * rather than a confident 0. Conflict detection is V2 (§B); this field
   * exists so the day Rust starts sending a number, nothing here changes.
   */
  conflicts: number | null;
  /** Machine-wide load for this tick. */
  system: SystemTelemetry;
  /** What this tick cost. */
  timing: ScanTiming;
  /**
   * Which version of the Developer Registry classified the services in this
   * snapshot. Shipped so a classification someone disagrees with can be pinned
   * to a specific version of the tables rather than to "the app".
   */
  registryVersion: number;
}

/** What the UI is currently showing. */
export type LoadState =
  | { kind: 'loading' }
  | { kind: 'ready'; snapshot: Snapshot }
  | { kind: 'empty'; snapshot: Snapshot }
  /**
   * A scan failed. `stale` is the last good snapshot, if there was one — the
   * UI keeps rendering it behind a warning rather than blanking. Null means
   * the very first scan failed and there is nothing to show.
   */
  | { kind: 'error'; message: string; detail: string; stale: Snapshot | null };

/**
 * Payload for `terminate_process`.
 *
 * Both fields are required. The backend re-opens the PID, reads its creation
 * time, and refuses if it does not match — so a recycled PID cannot be killed
 * by a stale row in the UI.
 */
export interface TerminateRequest {
  pid: number;
  startedAt: string;
}

export type TerminateResult =
  | { kind: 'terminated' }
  | { kind: 'stale'; message: string }
  | { kind: 'denied' }
  | { kind: 'failed'; message: string };

export type ScreenId =
  | 'overview'
  | 'services'
  | 'processes'
  | 'ports'
  | 'projects'
  | 'logs'
  | 'docker'
  | 'wsl'
  | 'settings';

/**
 * The three first-class themes.
 *
 * Explicit selection only — there is deliberately no `system` option. A window
 * that repaints itself because the OS crossed into dark mode mid-scan is a
 * surprise in a monitoring tool, and `local-dark` is the look the app is
 * designed around rather than a fallback.
 *
 * The value is written to `data-theme` on the document element verbatim, so it
 * must stay in sync with the blocks in `src/index.css`.
 */
export type Theme = 'local-dark' | 'dark' | 'light';

/**
 * The global presentation mode.
 *
 * Developer shows the services the Developer Registry classified as
 * development work, the processes that own them and the sockets they hold —
 * and nothing else. System shows everything the backend can observe.
 *
 * It is a *view* setting: the Snapshot the backend produces is identical
 * either way, and all narrowing happens in one place (`src/lib/view.ts`).
 */
export type AppMode = 'developer' | 'system';
