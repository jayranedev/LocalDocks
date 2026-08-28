import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS, __test } from './settings';

/**
 * Only the validation is covered. Stored JSON is hand-editable and survives
 * across versions, so the job of `coerce` is to make sure nothing invalid ever
 * reaches the UI. Reading and writing localStorage itself is the browser's
 * problem, not ours.
 */
const { coerce } = __test;

describe('settings coercion', () => {
  it('falls back to defaults for junk input', () => {
    expect(coerce(null)).toEqual(DEFAULT_SETTINGS);
    expect(coerce(undefined)).toEqual(DEFAULT_SETTINGS);
    expect(coerce('nonsense')).toEqual(DEFAULT_SETTINGS);
    expect(coerce(42)).toEqual(DEFAULT_SETTINGS);
    expect(coerce([])).toEqual(DEFAULT_SETTINGS);
  });

  it('accepts a valid stored object', () => {
    expect(coerce({ theme: 'dark', intervalMs: 2000 })).toEqual({
      theme: 'dark',
      intervalMs: 2000,
    });
  });

  it('rejects an unknown theme without discarding a valid interval', () => {
    expect(coerce({ theme: 'solarized', intervalMs: 500 })).toEqual({
      theme: DEFAULT_SETTINGS.theme,
      intervalMs: 500,
    });
  });

  it('rejects an interval that is not one of the offered choices', () => {
    expect(coerce({ theme: 'light', intervalMs: 17 })).toEqual({
      theme: 'light',
      intervalMs: DEFAULT_SETTINGS.intervalMs,
    });
    expect(coerce({ theme: 'light', intervalMs: '1000' })).toEqual({
      theme: 'light',
      intervalMs: DEFAULT_SETTINGS.intervalMs,
    });
  });

  it('ignores extra keys rather than passing them through', () => {
    expect(coerce({ theme: 'dark', intervalMs: 1000, rogue: true })).toEqual({
      theme: 'dark',
      intervalMs: 1000,
    });
  });
});
