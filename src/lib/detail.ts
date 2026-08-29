import type { Endpoint, ProcessId, Relevance, Snapshot } from '../types';

/**
 * Everything the shared detail panel needs, assembled from a snapshot.
 *
 * Services, Processes and Ports all address the same thing — a process — so
 * they all resolve to this one shape and render the same panel. The screens
 * differ only in what they pass as `highlight`.
 *
 * Pure and snapshot-driven, so it is trivially testable and cannot drift from
 * whatever the sampler last produced.
 */
export interface DetailTarget {
  processId: ProcessId;
  pid: number;
  parentPid: number;
  startedAt: string;
  processName: string;
  /** Service label when the process is a service, else the process name. */
  title: string;
  /** Framework chip, when project context is known. */
  badge: string | null;
  endpoints: Endpoint[];
  cpuPercent: number;
  memoryBytes: number;
  uptimeSeconds: number;
  threadCount: number;
  isService: boolean;
  /**
   * How the Developer Registry classified this service, and why.
   *
   * `null` when the process is not a service — there is nothing to classify,
   * and inventing an "unknown" would imply the registry had been consulted and
   * had no answer, which is a different claim.
   */
  classification: { relevance: Relevance; reason: string } | null;
  /** The socket the user clicked, when they arrived from the Ports screen. */
  highlight: { port: number; address: string } | null;
}

export function buildDetailTarget(
  snapshot: Snapshot,
  processId: ProcessId,
  highlight: { port: number; address: string } | null = null,
): DetailTarget | null {
  const process = snapshot.processes.find((p) => p.id === processId);
  if (!process) return null;

  const service = snapshot.services.find((s) => s.id === processId) ?? null;

  /* Prefer the service's endpoint list. Fall back to reconstructing it from
     the port table, so a process that holds a socket but did not qualify as a
     service still shows its endpoints. */
  const endpoints: Endpoint[] =
    service?.endpoints ??
    snapshot.ports
      .filter((p) => p.processId === processId)
      .map((p) => ({ protocol: p.protocol, address: p.address, port: p.port }));

  return {
    processId,
    pid: process.pid,
    parentPid: process.parentPid,
    startedAt: process.startedAt,
    processName: process.name,
    title: service?.label ?? process.name,
    badge: service?.framework ?? null,
    endpoints,
    cpuPercent: process.cpuPercent,
    memoryBytes: process.memoryBytes,
    uptimeSeconds: process.uptimeSeconds,
    threadCount: process.threadCount,
    isService: process.isService,
    classification: service
      ? { relevance: service.relevance, reason: service.relevanceReason }
      : null,
    highlight,
  };
}


/**
 * Whether an endpoint has a localhost URL worth opening.
 *
 * Only TCP on a loopback or wildcard address — a UDP socket has no URL, and
 * one bound to an external interface is not "localhost".
 */
export function isLoopbackTcp(endpoint: Endpoint): boolean {
  if (endpoint.protocol !== 'TCP') return false;
  return (
    endpoint.address === '127.0.0.1' ||
    endpoint.address === '[::1]' ||
    endpoint.address === '0.0.0.0' ||
    endpoint.address === '[::]'
  );
}

/** The port a detail panel should offer actions for. */
export function actionablePort(target: DetailTarget): number | null {
  if (target.highlight) {
    const match = target.endpoints.find(
      (e) => e.port === target.highlight!.port && e.address === target.highlight!.address,
    );
    return match && isLoopbackTcp(match) ? match.port : null;
  }
  const openable = target.endpoints.filter(isLoopbackTcp);
  if (openable.length === 0) return null;
  return openable.reduce((lo, e) => (e.port < lo ? e.port : lo), openable[0].port);
}
