/**
 * The Phantom School Manager mark: a gold crest around a P, on an indigo tile.
 *
 * The same drawing as src-tauri/icons/source.svg, which every window and
 * installer icon is generated from (`npx tauri icon src-tauri/icons/source.svg`),
 * so the taskbar icon and the mark inside the app are the same object.
 *
 * Below 24px the crest is dropped and the P fills the tile: a crest thin
 * enough to fit at that size turns into a gold smudge.
 */

import { useId } from "react";

export const APP_NAME = "Phantom School Manager";

export function BrandMark({ size = 36 }: { size?: number }) {
  // Gradient ids must be unique when the mark appears more than once.
  const id = useId().replace(/:/g, "");
  const crest = size >= 24;

  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id={`${id}-tile`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#1E1B4B" />
          <stop offset="0.6" stopColor="#3730A3" />
          <stop offset="1" stopColor="#6D28D9" />
        </linearGradient>
        <radialGradient id={`${id}-sheen`} cx="0.22" cy="0.12" r="0.85">
          <stop offset="0" stopColor="#FFFFFF" stopOpacity="0.22" />
          <stop offset="0.6" stopColor="#FFFFFF" stopOpacity="0" />
        </radialGradient>
        <linearGradient id={`${id}-gold`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#FDE68A" />
          <stop offset="1" stopColor="#F59E0B" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="15" fill={`url(#${id}-tile)`} />
      <rect width="64" height="64" rx="15" fill={`url(#${id}-sheen)`} />
      {crest ? (
        <>
          <path
            d="M32 9 L50 15 V31 C50 42 42 50 32 55 C22 50 14 42 14 31 V15 Z"
            fill="none"
            stroke={`url(#${id}-gold)`}
            strokeWidth="2.4"
            strokeLinejoin="round"
          />
          <path
            d="M26 44 V20 H34.5 A7.75 7.75 0 0 1 34.5 35.5 H26"
            fill="none"
            stroke="#FFFFFF"
            strokeWidth="6"
            strokeLinejoin="round"
          />
        </>
      ) : (
        <path
          d="M23 51 V14 H35 A10.5 10.5 0 0 1 35 35 H23"
          fill="none"
          stroke="#FFFFFF"
          strokeWidth="9"
          strokeLinejoin="round"
        />
      )}
    </svg>
  );
}
