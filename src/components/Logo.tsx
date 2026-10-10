/**
 * The Termbook mark: a bound book with three tabs on its edge, one for each
 * term, in white on an ink tile. Flat and single-coloured on purpose, so it
 * sits quietly beside whatever accent colour a school chooses.
 *
 * The same drawing as src-tauri/icons/source.svg, which every window and
 * installer icon is generated from (`npx tauri icon src-tauri/icons/source.svg`),
 * so the taskbar icon and the mark inside the app are the same object.
 */

export const APP_NAME = "Termbook";

const INK = "#1B2340";

export function BrandMark({ size = 36 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" focusable="false">
      <rect width="64" height="64" rx="14" fill={INK} />
      <rect x="17" y="13" width="26" height="38" rx="3" fill="none" stroke="#FFFFFF" strokeWidth="3.2" />
      <path d="M23.5 13 V51" stroke="#FFFFFF" strokeWidth="3.2" />
      <rect x="43" y="17.5" width="5.5" height="7.5" rx="1.6" fill="#FFFFFF" />
      <rect x="43" y="28.25" width="5.5" height="7.5" rx="1.6" fill="#FFFFFF" />
      <rect x="43" y="39" width="5.5" height="7.5" rx="1.6" fill="#FFFFFF" />
    </svg>
  );
}
