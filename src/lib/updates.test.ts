import { describe, expect, it } from 'vitest';
import { isCheckDue, RECHECK_INTERVAL_MS } from './updates';

/**
 * The scheduling rule, which is the only part of update checking that is pure
 * enough to test here. Whether a version is worth offering is decided in Rust
 * (`logic::release`), where the semver comparison and the no-prerelease and
 * no-downgrade rules live with their own tests.
 */
describe('isCheckDue', () => {
  const NOW = 1_800_000_000_000;

  it('checks on a machine that has never checked', () => {
    expect(isCheckDue(null, NOW)).toBe(true);
  });

  it('does not check again straight away', () => {
    expect(isCheckDue(NOW, NOW)).toBe(false);
    expect(isCheckDue(NOW - 60_000, NOW)).toBe(false);
  });

  it('does not check eleven times in an afternoon', () => {
    // Four launches over four hours, all after one real check.
    const lastChecked = NOW - 4 * 60 * 60 * 1000;
    expect(isCheckDue(lastChecked, NOW)).toBe(false);
  });

  it('checks once the interval has elapsed', () => {
    expect(isCheckDue(NOW - RECHECK_INTERVAL_MS, NOW)).toBe(true);
    expect(isCheckDue(NOW - RECHECK_INTERVAL_MS - 1, NOW)).toBe(true);
  });

  it('is not fooled one millisecond short of the interval', () => {
    expect(isCheckDue(NOW - RECHECK_INTERVAL_MS + 1, NOW)).toBe(false);
  });

  it('treats a timestamp from the future as due', () => {
    // Clocks go backwards: a timezone change, an NTP correction, a restored
    // disk image. A stored future value must not suppress every check until
    // real time catches up with it.
    expect(isCheckDue(NOW + RECHECK_INTERVAL_MS, NOW)).toBe(true);
  });

  it('treats a corrupted timestamp as due', () => {
    expect(isCheckDue(Number.NaN, NOW)).toBe(true);
    expect(isCheckDue(Number.POSITIVE_INFINITY, NOW)).toBe(true);
  });
});
