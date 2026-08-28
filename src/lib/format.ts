import type { Endpoint, FieldState } from '../types';

/** 148897792 -> "142 MB". Binary units, since that is what Task Manager shows. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${Math.round(mb)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function formatCpu(percent: number): string {
  return `${percent.toFixed(1)}%`;
}

/** 4342 -> "1h 12m". Coarse on purpose: nobody needs seconds after an hour. */
export function formatUptime(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function formatClock(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

/** Seconds since an ISO timestamp, for the "scanned N ago" readout. */
export function secondsSince(iso: string): number {
  return Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
}

/** The port a service is best known by — its lowest. */
export function primaryPort(endpoints: Endpoint[]): number | null {
  if (endpoints.length === 0) return null;
  return endpoints.reduce((lo, e) => (e.port < lo ? e.port : lo), endpoints[0].port);
}

/**
 * True when the same port is bound on both IPv4 and IPv6.
 *
 * Worth surfacing: it is the single most common reason a naive port table
 * shows one dev server twice.
 */
export function isDualStack(endpoints: Endpoint[]): boolean {
  const byPort = new Map<number, Set<string>>();
  for (const e of endpoints) {
    const family = e.address.startsWith('[') ? 'v6' : 'v4';
    const set = byPort.get(e.port) ?? new Set<string>();
    set.add(family);
    byPort.set(e.port, set);
  }
  return [...byPort.values()].some((s) => s.size > 1);
}

/** localhost URL for a port, for "Open in browser" and "Copy URL". */
export function localUrl(port: number): string {
  return `http://localhost:${port}`;
}

/** Render a possibly-denied field without ever showing a blank. */
export function fieldText(field: FieldState<string>): string {
  switch (field.kind) {
    case 'ok':
      return field.value;
    case 'denied':
      return 'Requires elevation — process owned by another account';
    case 'unavailable':
      return 'Unavailable';
  }
}

export function isFieldOk(field: FieldState<string>): boolean {
  return field.kind === 'ok';
}
