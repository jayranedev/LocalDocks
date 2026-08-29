import type { AppMode } from '../types';
import { MODE_HINTS, MODE_LABELS } from '../lib/view';

/**
 * The global Developer/System switch.
 *
 * A real switch, not a segmented control: one control with two states, a thumb
 * that slides, and the label riding inside the track. Off is System, on is
 * Developer.
 *
 * Three things carry the state, so none of them has to carry it alone:
 *
 *   1. **Position** — the thumb sits left for System, right for Developer.
 *   2. **Text** — the track spells out which mode is active.
 *   3. **Semantics** — `role="switch"` with `aria-checked`, so a screen reader
 *      announces it as a switch that is on or off rather than as a button.
 *
 * Colour is the fourth signal and deliberately not load-bearing: the control
 * still reads correctly in greyscale.
 *
 * Every colour is a semantic token, so the switch follows Local Dark, Dark and
 * Light without knowing they exist. Contrast was measured in all three: the
 * label clears AA against its track everywhere (5.5–10.1), the thumb stays
 * distinct from both tracks, and the track stays distinct from the title bar.
 */

interface Props {
  mode: AppMode;
  onChange: (mode: AppMode) => void;
}

const WIDTH = 128;
const HEIGHT = 26;
const THUMB = 20;
const INSET = 3;

export function ModeSwitch({ mode, onChange }: Props) {
  const developer = mode === 'developer';
  const toggle = () => onChange(developer ? 'system' : 'developer');

  return (
    <button
      type="button"
      role="switch"
      aria-checked={developer}
      aria-label="Developer mode"
      title={`${MODE_LABELS[mode]} mode — ${MODE_HINTS[mode]}`}
      onClick={toggle}
      onKeyDown={(e) => {
        /* Space and Enter come free with <button>. Arrow keys are the
           convention for a switch, and they set a side rather than toggling,
           so holding one does not oscillate. */
        if (e.key === 'ArrowRight') {
          e.preventDefault();
          onChange('developer');
        }
        if (e.key === 'ArrowLeft') {
          e.preventDefault();
          onChange('system');
        }
      }}
      style={{ width: WIDTH, height: HEIGHT }}
      className={`ld-switch relative flex-none rounded-full border ${
        developer
          ? 'border-transparent bg-accent'
          : 'border-border-strong bg-border-strong'
      }`}
    >
      {/* The label sits on the side the thumb is not, so they never overlap. */}
      <span
        className={`pointer-events-none absolute inset-y-0 flex items-center text-[11px] font-medium tracking-[0.01em] ${
          developer
            ? 'left-0 pl-[11px] text-accent-contrast'
            : 'right-0 pr-[11px] text-primary'
        }`}
      >
        {MODE_LABELS[mode]}
      </span>

      {/* The thumb. A light physical material in every theme, lifted off the
          track by a shadow so its edge is defined by form rather than fill. */}
      <span
        aria-hidden="true"
        className="ld-switch-thumb pointer-events-none absolute top-1/2 rounded-full"
        style={{
          width: THUMB,
          height: THUMB,
          left: developer ? WIDTH - THUMB - INSET : INSET,
        }}
      />
    </button>
  );
}
