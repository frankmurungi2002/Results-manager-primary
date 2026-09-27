/** The Results Manager mark: a ring of office enclosing a rising stair.
 *
 * Two things here are deliberate and were arrived at by cutting the real icon
 * and looking at it (scripts/render_icons.py, which produces src-tauri/icons):
 *
 *  - The stair is stroked with BUTT caps and MITRE joins. Round joins at this
 *    weight swallow the right angles and the staircase becomes an S-squiggle.
 *  - Below 20px the ring is dropped and the stair fills the box instead. A
 *    ring heavy enough to be visible at that size spends most of the box on
 *    white and leaves the stair nowhere to live.
 *
 * Keep this in step with render_icons.py — the icon in the taskbar and the
 * mark in the window should be the same object.
 */

const RING_RADIUS = 24.5;

/** The stair as it sits inside the ring, and as it fills the box without one. */
const STAIR_INSET = "M19 40 H27.5 V32 H36.5 V24 H45";
const STAIR_FULL = "M15 44 H27 V32 H39 V20 H50";

export function SealMark({
  size = 18,
  stair = "var(--gold-300)",
}: {
  size?: number;
  /** The stair colour. Pass "currentColor" for a one-colour mark. */
  stair?: string;
}) {
  const ringed = size >= 20;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      aria-hidden="true"
      focusable="false"
    >
      {ringed && (
        <circle
          cx="32"
          cy="32"
          r={RING_RADIUS}
          fill="none"
          stroke="currentColor"
          strokeWidth={size >= 40 ? 5.5 : 6.5}
        />
      )}
      <path
        d={ringed ? STAIR_INSET : STAIR_FULL}
        fill="none"
        stroke={stair}
        strokeWidth={ringed ? 6.5 : 9}
        strokeLinecap="butt"
        strokeLinejoin="miter"
      />
    </svg>
  );
}
