/**
 * The sign-in screen's four built-in pictures, drawn inline so the installer
 * carries no image files and the screen needs no internet. A school can
 * replace any of them with its own photo (Settings → School → Sign-in
 * pictures).
 */

import { LoginIllustration } from "./LoginIllustration";

const INK = "#1c2340";

/** 1 — two learners reading, on warm sand. */
export function SceneLearners() {
  return (
    <div className="scene" style={{ background: "linear-gradient(160deg, #f3e6d3, #e9d6bd)" }}>
      <div
        className="scene-art"
        style={{ ["--login-bush" as string]: "#e6d3b8", ["--login-cloud" as string]: "#f8efe2" }}
      >
        <LoginIllustration className="scene-svg" />
      </div>
    </div>
  );
}

/** 2 — a stack of exercise books, an apple and a pencil pot, on soft blue. */
export function SceneBooks() {
  return (
    <div className="scene" style={{ background: "linear-gradient(160deg, #d9e5ef, #c3d4e3)" }}>
      <svg className="scene-svg" viewBox="0 0 560 520" role="img" aria-label="Exercise books, an apple and pencils">
        <ellipse cx="280" cy="486" rx="220" ry="18" fill="#9fb4c9" opacity="0.45" />
        {/* books */}
        <g>
          <rect x="120" y="398" width="300" height="46" rx="8" fill="#2f6fd8" />
          <rect x="120" y="398" width="300" height="10" rx="5" fill="#5b8ff0" />
          <rect x="400" y="404" width="10" height="34" rx="3" fill="#eef3fb" />
          <rect x="138" y="352" width="276" height="46" rx="8" fill="#f2a33a" />
          <rect x="138" y="352" width="276" height="10" rx="5" fill="#f7c16e" />
          <rect x="394" y="358" width="10" height="34" rx="3" fill="#fff7ea" />
          <rect x="112" y="306" width="290" height="46" rx="8" fill="#e5536a" />
          <rect x="112" y="306" width="290" height="10" rx="5" fill="#f07f90" />
          <rect x="382" y="312" width="10" height="34" rx="3" fill="#fff0f2" />
          <rect x="150" y="262" width="250" height="44" rx="8" fill="#34a37a" />
          <rect x="150" y="262" width="250" height="10" rx="5" fill="#5cc49b" />
          <rect x="380" y="268" width="10" height="32" rx="3" fill="#eefaf5" />
          <rect x="170" y="214" width="220" height="48" rx="8" fill="#6d4fd8" />
          <rect x="170" y="214" width="220" height="10" rx="5" fill="#9178ea" />
          <rect x="210" y="232" width="110" height="10" rx="5" fill="#ffffff" opacity="0.7" />
        </g>
        {/* apple */}
        <g transform="translate(262 150)">
          <path d="M18 64c-22 0-36-20-36-42 0-18 12-32 28-32 6 0 10 3 14 3s8-3 14-3c16 0 28 14 28 32 0 22-14 42-36 42-4 0-7-2-12-2s-8 2-12 2z" fill="#e5363d" transform="translate(-6 0)" />
          <path d="M12 6c-2-10 2-18 10-22" stroke="#6b3e1c" strokeWidth="5" fill="none" strokeLinecap="round" />
          <path d="M22 -10c10-8 24-6 28 2-10 6-22 6-28-2z" fill="#3da35d" />
          <ellipse cx="0" cy="18" rx="7" ry="12" fill="#ffffff" opacity="0.35" />
        </g>
        {/* pencil pot */}
        <g transform="translate(430 250)">
          <rect x="-8" y="-86" width="12" height="96" rx="3" fill="#f6c343" transform="rotate(-10)" />
          <path d="M-8 -86l6-16 6 16z" fill="#f3d8b0" transform="rotate(-10)" />
          <rect x="14" y="-96" width="12" height="104" rx="3" fill="#2f6fd8" transform="rotate(8)" />
          <path d="M14 -96l6-16 6 16z" fill="#f3d8b0" transform="rotate(8)" />
          <rect x="-4" y="-70" width="10" height="80" rx="3" fill="#e5536a" />
          <rect x="-26" y="0" width="72" height="118" rx="14" fill="#ffffff" />
          <rect x="-26" y="30" width="72" height="14" fill="#c9d7e6" />
        </g>
        {/* sparkles */}
        <g fill="#ffffff">
          <path d="M90 120l6 16 16 6-16 6-6 16-6-16-16-6 16-6z" opacity="0.9" />
          <path d="M470 90l4 10 10 4-10 4-4 10-4-10-10-4 10-4z" opacity="0.8" />
        </g>
      </svg>
    </div>
  );
}

/** 3 — a graduation cap on a rolled certificate, on lavender. */
export function SceneGraduation() {
  return (
    <div className="scene" style={{ background: "linear-gradient(160deg, #e3ddf1, #cfc6e6)" }}>
      <svg className="scene-svg" viewBox="0 0 560 520" role="img" aria-label="A graduation cap and certificate">
        <ellipse cx="280" cy="470" rx="200" ry="18" fill="#a99fc9" opacity="0.45" />
        {/* certificate scroll */}
        <g transform="rotate(-8 280 400)">
          <rect x="110" y="372" width="340" height="62" rx="31" fill="#fbf6ea" />
          <ellipse cx="110" cy="403" rx="22" ry="31" fill="#efe4c9" />
          <ellipse cx="450" cy="403" rx="22" ry="31" fill="#efe4c9" />
          <rect x="266" y="368" width="30" height="70" fill="#d7263d" />
          <path d="M268 438l-10 34 22-14 22 14-10-34z" fill="#b51d31" />
        </g>
        {/* cap */}
        <g>
          <path d="M280 138l200 74-200 74-200-74z" fill={INK} />
          <path d="M280 138l200 74-200 74-200-74z" fill="url(#capShine)" />
          <path d="M170 246v70c0 30 50 52 110 52s110-22 110-52v-70l-110 40z" fill="#2b3563" />
          <path d="M280 212l150 -8" stroke="#f6c343" strokeWidth="5" />
          <path d="M430 204v96" stroke="#f6c343" strokeWidth="5" strokeLinecap="round" />
          <path d="M422 300h16l6 38h-28z" fill="#f6c343" />
          <circle cx="280" cy="212" r="12" fill="#f6c343" />
        </g>
        {/* stars */}
        <g fill="#f6c343">
          <path d="M110 100l8 20 20 8-20 8-8 20-8-20-20-8 20-8z" />
          <path d="M470 70l6 14 14 6-14 6-6 14-6-14-14-6 14-6z" />
          <path d="M60 300l5 12 12 5-12 5-5 12-5-12-12-5 12-5z" opacity="0.8" />
        </g>
        <defs>
          <linearGradient id="capShine" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="#ffffff" stopOpacity="0.18" />
            <stop offset="0.6" stopColor="#ffffff" stopOpacity="0" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}

/** 4 — a report card with a gold medal, on warm stone. */
export function SceneReport() {
  return (
    <div className="scene" style={{ background: "linear-gradient(160deg, #eee3cf, #dccbb0)" }}>
      <svg className="scene-svg" viewBox="0 0 560 520" role="img" aria-label="A report card and a medal">
        <ellipse cx="270" cy="484" rx="200" ry="16" fill="#b9a585" opacity="0.45" />
        {/* the card */}
        <g transform="rotate(-6 260 300)">
          <rect x="130" y="96" width="260" height="360" rx="18" fill="#ffffff" />
          <rect x="130" y="96" width="260" height="64" rx="18" fill="#4f46e5" />
          <rect x="130" y="140" width="260" height="20" fill="#4f46e5" />
          <rect x="156" y="116" width="120" height="12" rx="6" fill="#ffffff" opacity="0.85" />
          <rect x="156" y="136" width="80" height="8" rx="4" fill="#ffffff" opacity="0.55" />
          {[0, 1, 2, 3, 4].map((row) => (
            <g key={row} transform={`translate(0 ${row * 44})`}>
              <rect x="156" y="188" width="110" height="10" rx="5" fill="#e3e6ef" />
              <rect x="156" y="204" width="70" height="8" rx="4" fill="#eef0f6" />
              <rect x="318" y="186" width="46" height="24" rx="12" fill={["#dcf5e6", "#dff1ff", "#dcf5e6", "#fff1c9", "#dcf5e6"][row]} />
              <text x="341" y="203" textAnchor="middle" fontSize="13" fontWeight="800" fill={["#1d7a45", "#1f6fb2", "#1d7a45", "#a86b00", "#1d7a45"][row]} fontFamily="system-ui, sans-serif">
                {["D1", "D2", "D1", "C3", "D1"][row]}
              </text>
            </g>
          ))}
          <rect x="156" y="412" width="90" height="10" rx="5" fill="#e3e6ef" />
        </g>
        {/* medal */}
        <g transform="translate(410 330)">
          <path d="M-30 -120l22 70h24l-22-70z" fill="#2f6fd8" />
          <path d="M30 -120l-22 70h-24l22-70z" fill="#e5536a" />
          <circle cx="0" cy="0" r="54" fill="#f6c343" />
          <circle cx="0" cy="0" r="40" fill="#f9d56e" />
          <path d="M0 -22l7 15 16 2-12 11 3 16-14-8-14 8 3-16-12-11 16-2z" fill="#d99a14" />
        </g>
        {/* pencil */}
        <g transform="rotate(35 120 420)">
          <rect x="60" y="410" width="150" height="18" rx="4" fill="#f6c343" />
          <rect x="60" y="410" width="20" height="18" rx="3" fill="#e5536a" />
          <path d="M210 410l26 9-26 9z" fill="#f3d8b0" />
          <path d="M228 416l8 3-8 3z" fill={INK} />
        </g>
      </svg>
    </div>
  );
}

export const BUILT_IN_SCENES = [SceneLearners, SceneBooks, SceneGraduation, SceneReport];
