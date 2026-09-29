/**
 * The sign-in screen's illustration: two learners, drawn inline so the
 * installer carries no image files and the page works with no internet.
 */

const SKIN_BOY = "#c9825a";
const SKIN_BOY_SHADE = "#b36f4b";
const SKIN_GIRL = "#f4c27a";
const SKIN_GIRL_SHADE = "#e5ad62";
const NAVY = "#17224d";

export function LoginIllustration({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 560 520"
      role="img"
      aria-label="Two learners reading"
      preserveAspectRatio="xMidYMax meet"
    >
      {/* Clouds */}
      <g fill="var(--login-cloud)">
        <path d="M40 212c0-12 10-21 22-21 4-13 16-22 30-22 15 0 27 10 31 23 11 1 19 9 19 20H40z" />
        <path d="M352 196c0-10 8-18 18-18 4-11 14-18 26-18 13 0 23 8 26 20 9 1 16 8 16 16H352z" />
        <path d="M440 256c0-7 6-13 13-13 3-8 10-13 19-13 9 0 17 6 19 14 7 0 12 6 12 12h-63z" />
      </g>

      {/* Foliage behind the learners */}
      <g fill="var(--login-bush)">
        <path d="M30 470c10-70 40-120 70-150 6 40 2 80-8 120 20-60 55-110 95-130-4 60-20 110-40 160z" />
        <path d="M150 470c20-90 70-160 120-190-2 70-20 130-45 190z" />
        <path d="M250 470c15-60 50-110 95-140 0 55-15 100-35 140z" />
        <path d="M330 470c10-70 45-140 100-170-6 70-25 125-50 170z" />
        <path d="M410 470c14-50 45-95 90-120-4 50-18 90-38 120z" />
        <rect x="20" y="452" width="510" height="22" rx="11" />
      </g>

      {/* ---- The boy -------------------------------------------------- */}
      <g>
        {/* legs */}
        <rect x="146" y="398" width="28" height="106" rx="12" fill={SKIN_BOY} />
        <rect x="184" y="398" width="28" height="106" rx="12" fill={SKIN_BOY_SHADE} />
        {/* shoes */}
        <ellipse cx="160" cy="506" rx="22" ry="9" fill="#f6d45a" />
        <ellipse cx="198" cy="506" rx="22" ry="9" fill="#f6d45a" />
        {/* shorts */}
        <path d="M130 322h100l-4 96h-44l-4-40-4 40h-44z" fill={NAVY} />
        <path d="M205 340c8 20 8 40 2 62" stroke="#3a5bd0" strokeWidth="3" fill="none" strokeLinecap="round" />
        {/* backpack */}
        <rect x="96" y="232" width="48" height="118" rx="20" fill="#4aa6dc" />
        <rect x="104" y="296" width="30" height="34" rx="8" fill="#3c8fc3" />
        {/* shirt */}
        <rect x="118" y="226" width="118" height="120" rx="34" fill="#eceff4" />
        {/* strap */}
        <path d="M150 232c-10 30-12 70-8 110" stroke="#4aa6dc" strokeWidth="14" fill="none" strokeLinecap="round" />
        {/* neck */}
        <rect x="164" y="206" width="26" height="30" rx="10" fill={SKIN_BOY_SHADE} />
        {/* arm and book */}
        <path d="M222 250c18 18 26 44 24 70" stroke={SKIN_BOY} strokeWidth="22" fill="none" strokeLinecap="round" />
        <g transform="rotate(-22 262 300)">
          <rect x="228" y="262" width="66" height="84" rx="6" fill="#8fdccf" />
          <rect x="228" y="262" width="10" height="84" rx="4" fill="#6fc6b8" />
          <path d="M270 262l14-22 10 22" fill="#8fdccf" stroke="#6fc6b8" strokeWidth="3" strokeLinejoin="round" />
        </g>
        <circle cx="246" cy="320" r="13" fill={SKIN_BOY} />
        {/* head */}
        <circle cx="140" cy="166" r="14" fill={SKIN_BOY_SHADE} />
        <circle cx="178" cy="162" r="52" fill={SKIN_BOY} />
        {/* hair */}
        <g fill={NAVY}>
          <circle cx="132" cy="138" r="24" />
          <circle cx="150" cy="112" r="26" />
          <circle cx="182" cy="100" r="28" />
          <circle cx="214" cy="112" r="24" />
          <circle cx="226" cy="138" r="18" />
          <circle cx="128" cy="166" r="14" />
        </g>
        {/* face */}
        <circle cx="194" cy="158" r="5" fill={NAVY} />
        <circle cx="222" cy="156" r="5" fill={NAVY} />
        <path d="M188 144c5-4 11-4 16 0M216 142c5-4 10-4 14 0" stroke={NAVY} strokeWidth="3" fill="none" strokeLinecap="round" />
        <circle cx="180" cy="180" r="9" fill="#ec7f5b" opacity="0.8" />
        <circle cx="228" cy="178" r="7" fill="#ec7f5b" opacity="0.8" />
        <path d="M198 186c8 7 18 7 24 0" stroke={NAVY} strokeWidth="3" fill="none" strokeLinecap="round" />
      </g>

      {/* ---- The girl ------------------------------------------------- */}
      <g>
        {/* legs */}
        <rect x="372" y="394" width="16" height="84" rx="7" fill={SKIN_GIRL} />
        <rect x="396" y="394" width="16" height="84" rx="7" fill={SKIN_GIRL_SHADE} />
        <path d="M366 482h26v-8c0-4-4-6-8-6h-10c-5 0-8 4-8 8z" fill={NAVY} />
        <path d="M392 482h26v-8c0-4-4-6-8-6h-10c-5 0-8 4-8 8z" fill={NAVY} />
        {/* dress */}
        <path d="M366 300h52l30 104h-112z" fill="#ef3348" />
        <path d="M368 300h48l-6 26h-36z" fill={NAVY} />
        {/* neck */}
        <rect x="384" y="284" width="16" height="20" rx="6" fill={SKIN_GIRL_SHADE} />
        {/* hands */}
        <circle cx="346" cy="344" r="10" fill={SKIN_GIRL} />
        <circle cx="438" cy="344" r="10" fill={SKIN_GIRL} />
        {/* open book */}
        <path d="M392 330l-50 -12v44l50 12z" fill="#6cb7e6" />
        <path d="M392 330l50 -12v44l-50 12z" fill="#5aa7d9" />
        <path d="M392 330v44" stroke="#3f8cc0" strokeWidth="3" />
        {/* hair, behind the face */}
        <g fill="#9b4d45">
          <circle cx="352" cy="248" r="22" />
          <circle cx="432" cy="248" r="22" />
          <circle cx="352" cy="274" r="18" />
          <circle cx="432" cy="274" r="18" />
          <ellipse cx="392" cy="236" rx="46" ry="44" />
        </g>
        {/* face */}
        <ellipse cx="392" cy="252" rx="34" ry="38" fill={SKIN_GIRL} />
        <path d="M358 232c10-24 58-26 68 0-14-10-52-10-68 0z" fill="#9b4d45" />
        <path d="M374 252c4 4 10 4 14 0M396 252c4 4 10 4 14 0" stroke={NAVY} strokeWidth="2.6" fill="none" strokeLinecap="round" />
        <circle cx="372" cy="266" r="6" fill="#ee8b6a" opacity="0.7" />
        <circle cx="412" cy="266" r="6" fill="#ee8b6a" opacity="0.7" />
        <path d="M386 272c4 4 8 4 12 0" stroke={NAVY} strokeWidth="2.4" fill="none" strokeLinecap="round" />
      </g>
    </svg>
  );
}
