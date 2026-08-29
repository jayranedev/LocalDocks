import { describe, expect, it } from 'vitest';
import {
  fieldText,
  formatBytes,
  formatClock,
  formatCpu,
  formatUptime,
  isDualStack,
  isFieldOk,
  localUrl,
  primaryPort,
  formatRate,
  secondsSince,
} from './format';
import type { Endpoint } from '../types';

const tcp = (address: string, port: number): Endpoint => ({ protocol: 'TCP', address, port });
const MB = 1024 * 1024;

describe('formatBytes', () => {
  it('uses bytes below 1 KiB', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1023)).toBe('1023 B');
  });

  it('uses binary megabytes, matching Task Manager', () => {
    expect(formatBytes(142 * MB)).toBe('142 MB');
    expect(formatBytes(1024)).toBe('0 MB'); // 1 KiB rounds to 0 MB, not "1 KB"
  });

  it('switches to gigabytes at 1024 MB', () => {
    expect(formatBytes(1023 * MB)).toBe('1023 MB');
    expect(formatBytes(1024 * MB)).toBe('1.00 GB');
    expect(formatBytes(1536 * MB)).toBe('1.50 GB');
  });
});

describe('formatCpu', () => {
  it('always shows one decimal so columns do not jitter', () => {
    expect(formatCpu(0)).toBe('0.0%');
    expect(formatCpu(2.34)).toBe('2.3%');
    expect(formatCpu(100)).toBe('100.0%');
  });
});

describe('formatUptime', () => {
  it('is coarse on purpose — no seconds past a minute', () => {
    expect(formatUptime(0)).toBe('0s');
    expect(formatUptime(59)).toBe('59s');
    expect(formatUptime(60)).toBe('1m');
    expect(formatUptime(3599)).toBe('59m');
  });

  it('shows hours and minutes, then days and hours', () => {
    expect(formatUptime(4342)).toBe('1h 12m');
    expect(formatUptime(86_400)).toBe('1d 0h');
    expect(formatUptime(273_600)).toBe('3d 4h');
  });
});

describe('formatClock', () => {
  it('renders an em dash for an unparseable timestamp rather than "Invalid Date"', () => {
    expect(formatClock('not-a-date')).toBe('—');
    expect(formatClock('')).toBe('—');
  });

  it('renders something for a valid ISO timestamp', () => {
    expect(formatClock(new Date().toISOString())).not.toBe('—');
  });
});

describe('secondsSince', () => {
  it('never returns a negative age for a future timestamp', () => {
    const future = new Date(Date.now() + 60_000).toISOString();
    expect(secondsSince(future)).toBe(0);
  });

  it('measures elapsed time', () => {
    const past = new Date(Date.now() - 5000).toISOString();
    expect(secondsSince(past)).toBeGreaterThanOrEqual(4.9);
    expect(secondsSince(past)).toBeLessThan(6);
  });
});

describe('primaryPort', () => {
  it('returns null when a process holds no sockets', () => {
    expect(primaryPort([])).toBeNull();
  });

  it('picks the lowest port, whatever the order', () => {
    expect(primaryPort([tcp('127.0.0.1', 8000)])).toBe(8000);
    expect(primaryPort([tcp('127.0.0.1', 9229), tcp('127.0.0.1', 5173)])).toBe(5173);
  });
});

describe('isDualStack', () => {
  it('is false for a single socket', () => {
    expect(isDualStack([tcp('127.0.0.1', 5173)])).toBe(false);
  });

  it('detects the same port on both address families', () => {
    expect(isDualStack([tcp('127.0.0.1', 5173), tcp('[::1]', 5173)])).toBe(true);
  });

  it('is false for two different ports on the same family', () => {
    expect(isDualStack([tcp('127.0.0.1', 5173), tcp('127.0.0.1', 8000)])).toBe(false);
  });

  it('is false for two v6 sockets on the same port', () => {
    expect(isDualStack([tcp('[::1]', 5173), tcp('[::]', 5173)])).toBe(false);
  });
});

describe('localUrl', () => {
  it('builds a localhost URL', () => {
    expect(localUrl(5173)).toBe('http://localhost:5173');
  });
});

describe('fieldText / isFieldOk', () => {
  it('passes through a readable value', () => {
    const field = { kind: 'ok', value: 'C:\\node.exe' } as const;
    expect(fieldText(field)).toBe('C:\\node.exe');
    expect(isFieldOk(field)).toBe(true);
  });

  it('explains a denial instead of rendering blank', () => {
    const text = fieldText({ kind: 'denied' });
    expect(text).toContain('Requires elevation');
    expect(text.length).toBeGreaterThan(0);
    expect(isFieldOk({ kind: 'denied' })).toBe(false);
  });

  it('never returns an empty string for any variant', () => {
    expect(fieldText({ kind: 'unavailable' })).not.toBe('');
  });
});

describe('formatRate', () => {
  it('never rounds a measured rate away to zero', () => {
    // The defect this function exists for: 200 KB/s through the memory
    // formatter renders as "0 MB", which a reader cannot tell from an idle
    // link.
    expect(formatRate(200_000)).not.toMatch(/^0 /);
    expect(formatRate(1)).not.toMatch(/^0 /);
    expect(formatRate(0.4)).not.toMatch(/^0 /);
    expect(formatRate(1023)).not.toMatch(/^0 /);
  });

  it('only prints zero for a genuine zero', () => {
    expect(formatRate(0)).toBe('0 B/s');
    expect(formatRate(-5)).toBe('0 B/s');
  });

  it('steps down to the unit that keeps the number readable', () => {
    expect(formatRate(512)).toBe('512 B/s');
    expect(formatRate(2048)).toBe('2.0 KB/s');
    expect(formatRate(200_000)).toBe('195 KB/s');
    expect(formatRate(5 * 1024 * 1024)).toBe('5.0 MB/s');
    expect(formatRate(120 * 1024 * 1024)).toBe('120 MB/s');
    expect(formatRate(3 * 1024 ** 3)).toBe('3.0 GB/s');
  });

  it('always carries a unit', () => {
    for (const value of [0, 1, 999, 1024, 1e6, 1e9, 1e12]) {
      expect(formatRate(value)).toMatch(/(B|KB|MB|GB)\/s$/);
    }
  });
});
