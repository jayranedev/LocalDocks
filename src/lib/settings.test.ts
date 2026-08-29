import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS, MODES, THEMES, THEME_LABELS, __test } from './settings';

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
    expect(coerce({ theme: 'dark', intervalMs: 2000, mode: 'system' })).toEqual({
      theme: 'dark',
      intervalMs: 2000,
      mode: 'system',
    });
  });

  it('accepts every offered theme', () => {
    for (const theme of THEMES) {
      expect(coerce({ theme, intervalMs: 1000 }).theme).toBe(theme);
    }
  });

  it('migrates a settings file written before explicit theming', () => {
    // 'system' was a valid choice until the three named themes replaced it.
    // It must not survive into `data-theme`, where no CSS block matches it.
    expect(coerce({ theme: 'system', intervalMs: 1000 }).theme).toBe('local-dark');
  });

  it('rejects an unknown theme without discarding a valid interval', () => {
    expect(coerce({ theme: 'solarized', intervalMs: 500 })).toEqual({
      theme: DEFAULT_SETTINGS.theme,
      intervalMs: 500,
      mode: DEFAULT_SETTINGS.mode,
    });
  });

  it('defaults to the signature theme rather than a neutral one', () => {
    expect(DEFAULT_SETTINGS.theme).toBe('local-dark');
    expect(THEMES[0]).toBe('local-dark');
  });

  it('labels every theme it offers', () => {
    // A theme added to THEMES without a label would render as `undefined`.
    for (const theme of THEMES) {
      expect(THEME_LABELS[theme]).toBeTruthy();
    }
    expect(Object.keys(THEME_LABELS)).toHaveLength(THEMES.length);
  });

  it('rejects an interval that is not one of the offered choices', () => {
    expect(coerce({ theme: 'light', intervalMs: 17 })).toEqual({
      theme: 'light',
      intervalMs: DEFAULT_SETTINGS.intervalMs,
      mode: DEFAULT_SETTINGS.mode,
    });
    expect(coerce({ theme: 'light', intervalMs: '1000' })).toEqual({
      theme: 'light',
      intervalMs: DEFAULT_SETTINGS.intervalMs,
      mode: DEFAULT_SETTINGS.mode,
    });
  });

  it('defaults to Developer mode', () => {
    expect(DEFAULT_SETTINGS.mode).toBe('developer');
    expect(coerce({}).mode).toBe('developer');
  });

  it('accepts both modes', () => {
    for (const mode of MODES) {
      expect(coerce({ mode }).mode).toBe(mode);
    }
  });

  it('rejects an unknown mode without discarding the rest', () => {
    expect(coerce({ theme: 'light', intervalMs: 500, mode: 'wizard' })).toEqual({
      theme: 'light',
      intervalMs: 500,
      mode: DEFAULT_SETTINGS.mode,
    });
  });

  it('reads a settings file written before the mode existed', () => {
    // The common case on first run after an update: no `mode` key at all.
    expect(coerce({ theme: 'dark', intervalMs: 2000 })).toEqual({
      theme: 'dark',
      intervalMs: 2000,
      mode: 'developer',
    });
  });

  it('ignores extra keys rather than passing them through', () => {
    expect(coerce({ theme: 'dark', intervalMs: 1000, rogue: true })).toEqual({
      theme: 'dark',
      intervalMs: 1000,
      mode: DEFAULT_SETTINGS.mode,
    });
  });
});
