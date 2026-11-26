// Folio's icon system (uix-b1, 2026-11-20; contract next/app/DESIGN.md 3.6):
//   one grid (20 units, drawn inside 2.5–17.5), one round 1.5-unit stroke, round ends and joins, currentColor; shown at 16 or 20 px.
//   General shapes (arrows, chevrons, files, eye, lock, layers…) are Lucide's (ISC, ui/icons.lucide.ts, scaled 20/24 with
//   a 1.8 stroke so they weigh the same); publishing shapes (tools, frames, wraps, styles, fit, fill and stroke…) are Folio's
//   own on the same rules. Filled shapes only where the fill is the meaning (the selection arrow, colour chips, status badges).
import { createElement, useId } from 'react';
import { LUCIDE } from './icons.lucide';

const S = { fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinejoin: 'round' as const, strokeLinecap: 'round' as const };
const F = { fill: 'currentColor', stroke: 'none' };
/** a filled shape with softly rounded corners (a hairline of the same colour around it) */
const FR = { fill: 'currentColor', stroke: 'currentColor', strokeWidth: 0.8, strokeLinejoin: 'round' as const };

function lucide(name: string) {
  const shapes = LUCIDE[name];
  return (
    <g transform="scale(0.8333333)" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
      {shapes.map(([tag, attrs], i) => createElement(tag, { key: i, ...attrs }))}
    </g>
  );
}

/** the classic pointer: filled for Selection, open for Direct Selection */
const ARROW = 'M5.2 2.8v13.3l3.3-3.2 2.3 5.1 2.3-1-2.3-5h4.6z';
/** a small filled pointer in the lower right, for tools that act on a whole thing (page, free transform) */
const SMALL_ARROW = 'M11.2 9.8v7.4l1.8-1.7 1.4 3 1.3-.6-1.4-2.9h2.6z';
const LIST_LINES = 'M13 9h4.5M13 12.25h4.5M13 15.5h4.5';
const HEXAGON = 'M10 2.8l6.3 3.6v7.2L10 17.2l-6.3-3.6V6.4z';

const OWN: Record<string, (id: string) => React.ReactNode> = {
  // tools
  selection: () => <path d={ARROW} {...FR} />,
  direct: () => <path d={ARROW} {...S} />,
  page: () => (
    <>
      <path d="M3.5 3.5h7.5v11H3.5z" {...S} />
      <path d={SMALL_ARROW} {...FR} />
    </>
  ),
  gap: () => <path d="M3.5 3v14M16.5 3v14M7 10h6M8.8 8 7 10l1.8 2M11.2 8l1.8 2-1.8 2" {...S} />,
  collector: () => <path d="M3 12.5v3a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-3M10 3v8.5M6.8 8.3l3.2 3.2 3.2-3.2" {...S} />,
  typePath: () => <path d="M6.5 3.5h7M10 3.5v7.5M2.5 16c2.8-3 5.5-3 7.2-1.3s4.6 2.2 7.8-1.2" {...S} />,
  line: () => <path d="M4 16 16 4" {...S} />,
  rectFrame: () => <path d="M3 4.5h14v11H3zM3 4.5l14 11M17 4.5l-14 11" {...S} />,
  ellipseFrame: () => (
    <>
      <ellipse cx="10" cy="10" rx="7" ry="5.5" {...S} />
      <path d="M5 6.2 15 13.8M15 6.2 5 13.8" {...S} />
    </>
  ),
  polygonFrame: () => <path d={`${HEXAGON}M3.7 6.4l12.6 7.2M16.3 6.4 3.7 13.6`} {...S} />,
  rect: () => <path d="M3 4.5h14v11H3z" {...S} />,
  ellipse: () => <ellipse cx="10" cy="10" rx="7" ry="5.5" {...S} />,
  polygon: () => <path d={HEXAGON} {...S} />,
  freeTransform: () => (
    <>
      <path d="M3 4h9.5v9H3z" {...S} strokeLinecap="butt" strokeDasharray="2.2 1.8" />
      <path d={SMALL_ARROW} {...FR} />
    </>
  ),
  rotate: () => <path d="M16 10a6 6 0 1 1-1.8-4.3M16.2 3v3h-3" {...S} />,
  scale: () => <path d="M3.5 3.5h6v6h-6zM3.5 12.5v4h13v-13h-4M12 8l4.5-4.5M13 3.5h3.5V7" {...S} />,
  shear: () => <path d="M7 4.5h10l-4 11H3z" {...S} />,
  gradient: (id) => (
    <>
      <defs>
        <linearGradient id={`g${id}`} x1="0" x2="1">
          <stop offset="0" stopColor="currentColor" stopOpacity="0" />
          <stop offset="1" stopColor="currentColor" />
        </linearGradient>
      </defs>
      <rect x="3.5" y="3.5" width="13" height="13" rx="1.5" fill={`url(#g${id})`} {...{ stroke: 'currentColor', strokeWidth: 1.5 }} />
    </>
  ),
  gradientFeather: (id) => (
    <>
      <defs>
        <radialGradient id={`f${id}`}>
          <stop offset="0" stopColor="currentColor" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0" />
        </radialGradient>
      </defs>
      <rect x="3.5" y="3.5" width="13" height="13" rx="1.5" fill={`url(#f${id})`} stroke="currentColor" strokeWidth={1.5} strokeDasharray="2.2 1.8" />
    </>
  ),
  // fill / stroke / modes
  swapArrow: () => <path d="M5 5h7a2 2 0 0 1 2 2v6M11.5 10.5 14 13l2.5-2.5M7.5 2.5 5 5l2.5 2.5" {...S} />,
  container: () => <path d="M3.5 3.5h13v13h-13zM7 7h6v6H7z" {...S} />,
  textMode: () => <path d="M5.5 5h9M10 5v10M8 15h4" {...S} />,
  applyColor: () => <rect x="3.5" y="3.5" width="13" height="13" rx="1" {...F} />,
  applyGradient: (id) => (
    <>
      <defs>
        <linearGradient id={`a${id}`} x1="0" x2="1">
          <stop offset="0" stopColor="#fff" />
          <stop offset="1" stopColor="#000" />
        </linearGradient>
      </defs>
      <rect x="3.5" y="3.5" width="13" height="13" rx="1" fill={`url(#a${id})`} stroke="currentColor" strokeWidth={1} />
    </>
  ),
  applyNone: () => (
    <>
      <rect x="3.5" y="3.5" width="13" height="13" rx="1" fill="#fff" stroke="currentColor" strokeWidth={1} />
      <path d="M4.3 15.7 15.7 4.3" stroke="#e0302a" strokeWidth={1.6} strokeLinecap="round" />
    </>
  ),
  screenNormal: () => (
    <>
      <rect x="2.5" y="4" width="15" height="12" rx="1.5" {...S} />
      <path d="M6 7.5h8v5H6z" {...S} strokeWidth={1} strokeDasharray="1.6 1.4" />
    </>
  ),
  screenPreview: () => <rect x="2.5" y="4" width="15" height="12" rx="1.5" {...S} />,
  screenBleed: () => (
    <>
      <rect x="2.5" y="4" width="15" height="12" rx="1.5" {...S} strokeLinecap="butt" strokeDasharray="2.2 1.8" />
      <path d="M5.5 7h9v6h-9z" {...S} />
    </>
  ),
  // panels
  stroke: () => (
    <>
      <path d="M3 5h14" {...S} strokeWidth={1} />
      <path d="M3 9.5h14" {...S} strokeWidth={1.75} />
      <path d="M3 14.75h14" {...S} strokeWidth={3} strokeLinecap="butt" />
    </>
  ),
  color: () => (
    <>
      <circle cx="7.5" cy="7.8" r="4.3" {...S} />
      <circle cx="12.5" cy="7.8" r="4.3" {...S} />
      <circle cx="10" cy="12.3" r="4.3" {...S} />
    </>
  ),
  character: () => <path d="M4.5 16.5 10 3.5l5.5 13M6.6 11.5h6.8" {...S} />,
  paragraphStyles: () => <path d={`M5.5 15.5v-11h4.5M8 4.5v11M5.5 4.5a2.8 2.8 0 0 0 0 5.6${LIST_LINES}`} {...S} />,
  characterStyles: () => <path d={`M2.5 15.5 6.75 4.5h.5L11.5 15.5M4 11.6h6${LIST_LINES}`} {...S} />,
  objectStyles: () => <path d={`M3 4.5h7.5V11H3zM3 15.5h7.5${LIST_LINES}`} {...S} />,
  cellStyles: () => <path d={`M3 4.5h7.5V11H3zM3 7.75h7.5M6.75 4.5V11M3 15.5h7.5${LIST_LINES}`} {...S} />,
  tableStyles: () => <path d="M3 4h14v8.5H3zM3 7h14M3 9.75h14M8 4v8.5M12.5 4v8.5M3 16h14" {...S} />,
  table: () => <path d="M3 4.5h14v11H3zM3 8.5h14M3 12h14M8.5 4.5v11" {...S} />,
  align: () => <path d="M3.5 3v14M6.5 5.5h9v3h-9zM6.5 11.5h6v3h-6z" {...S} />,
  transform: () => <path d="M5.5 5.5h9v9h-9zM2.5 2.5l3 3M17.5 2.5l-3 3M2.5 17.5l3-3M17.5 17.5l-3-3" {...S} />,
  textWrap: () => <path d="M7.5 7.5h5v5h-5zM3 3.5h14M3 16.5h14M3 7.5h2M3 10h2M3 12.5h2M15 7.5h2M15 10h2M15 12.5h2" {...S} />,
  effects: () => <path d="M9.5 4H8.3a2 2 0 0 0-2 1.6L4.5 16M4.8 9.5h4.2M11 9.5l5 6.5M16 9.5l-5 6.5" {...S} />,
  // small chrome icons
  chevronDown: () => <path d="m5 7.5 5 5 5-5" {...S} />,
  chain: () => <path d="M7 7.5V6a3 3 0 0 1 6 0v1.5M7 12.5V14a3 3 0 0 0 6 0v-1.5M10 7.5v5" {...S} />,
  chainBroken: () => <path d="M7 7V6a3 3 0 0 1 6 0v1M7 13v1a3 3 0 0 0 6 0v-1M4 10h1.5M14.5 10H16" {...S} />,
  wrapNone: () => <path d="M3 4h14M3 7h14M3 10h14M3 13h14M3 16h14M7 6.5h6v7H7z" {...S} strokeWidth={1.25} />,
  wrapBox: () => <path d="M7 7h6v6H7zM3 4h14M3 16h14M3 7v6M17 7v6" {...S} />,
  wrapShape: () => <path d="M3 4h14M3 16h14M10 7a3 3 0 1 1 0 6 3 3 0 0 1 0-6zM3 8h3M3 12h3M14 8h3M14 12h3" {...S} />,
  wrapJump: () => <path d="M3 4h14M3 6.75h14M7 9.5h6v3.5H7zM3 16h14" {...S} />,
  fitContent: () => <path d="M3 3h14v14H3zM6.5 6.5h7v7h-7z" {...S} />,
  fitFrame: () => <path d="M5.5 5.5h9v9h-9zM2.5 2.5l3 3M17.5 2.5l-3 3M2.5 17.5l3-3M17.5 17.5l-3-3" {...S} />,
  fitProportional: () => <path d="M3 5h14v10H3zM6.5 5v10M13.5 5v10" {...S} />,
  fillFrame: () => <path d="M3 5h14v10H3zM3 8.3h14M3 11.7h14" {...S} />,
  center: () => <path d="M3 3h14v14H3zM7.5 10h5M10 7.5v5" {...S} />,
  cornerOptions: () => <path d="M4 16v-6a6 6 0 0 1 6-6h6" {...S} />,
  textFrameOptions: () => <path d="M3 4h14v12H3zM6 7.5h8M6 10.25h8M6 13h5" {...S} />,
  textJustify: () => <path d="M3 5h14M3 8.3h14M3 11.6h14M3 15h9" {...S} />,
  fx: () => <path d="M9.5 4H8.3a2 2 0 0 0-2 1.6L4.5 16M4.8 9.5h4.2M11 9.5l5 6.5M16 9.5l-5 6.5" {...S} />,
  capJoinButt: () => <path d="M3 10h9" stroke="currentColor" strokeWidth={5} />,
  capRound: () => <path d="M3 10h9" stroke="currentColor" strokeWidth={5} strokeLinecap="round" />,
  capSquare: () => <path d="M3 10h9" stroke="currentColor" strokeWidth={5} strokeLinecap="square" />,
  joinMiter: () => <path d="M4 15V5h11" fill="none" stroke="currentColor" strokeWidth={4} strokeLinejoin="miter" />,
  joinRound: () => <path d="M4 15V5h11" fill="none" stroke="currentColor" strokeWidth={4} strokeLinejoin="round" />,
  joinBevel: () => <path d="M4 15V5h11" fill="none" stroke="currentColor" strokeWidth={4} strokeLinejoin="bevel" />,
  strokeCenter: () => <><path d="M4 4h12v12H4z" fill="none" stroke="currentColor" strokeWidth={0.8} strokeDasharray="1.5 1.2" /><path d="M4 4h12v12H4z" fill="none" stroke="currentColor" strokeWidth={3} opacity={0.6} /></>,
  strokeInside: () => <><path d="M4 4h12v12H4z" fill="none" stroke="currentColor" strokeWidth={0.8} strokeDasharray="1.5 1.2" /><path d="M5.5 5.5h9v9h-9z" fill="none" stroke="currentColor" strokeWidth={3} opacity={0.6} /></>,
  strokeOutside: () => <><path d="M5.5 5.5h9v9h-9z" fill="none" stroke="currentColor" strokeWidth={0.8} strokeDasharray="1.5 1.2" /><path d="M4 4h12v12H4z" fill="none" stroke="currentColor" strokeWidth={3} opacity={0.6} /></>,
  spot: () => <circle cx="10" cy="10" r="4" {...F} />,
  process: () => <rect x="5" y="5" width="10" height="10" rx="1" {...F} />,
  cmyk: () => <path d="M4 4h6v6H4zM10 10h6v6h-6z" {...F} />,
  rgb: () => <path d="M4 10h12" stroke="currentColor" strokeWidth={6} />,
  noEdit: () => <path d="M4 16 16 4" {...S} />,
  missing: () => (
    <>
      <circle cx="10" cy="10" r="7" style={{ fill: 'var(--bad-fill)' }} />
      <path d="M10 6.3v4.6M10 13.6v.1" stroke="#fff" strokeWidth={1.8} strokeLinecap="round" />
    </>
  ),
  modified: () => (
    <>
      <path d="M8.7 3.7a1.5 1.5 0 0 1 2.6 0l6 10.6a1.5 1.5 0 0 1-1.3 2.2H4a1.5 1.5 0 0 1-1.3-2.2z" fill="#e8a33d" />
      <path d="M10 7.8v3.8M10 14v.1" stroke="#1b1300" strokeWidth={1.7} strokeLinecap="round" />
    </>
  ),
  embedded: () => <path d="M4 4h12v12H4zM7 8.25h6M7 11.75h6" {...S} />,
};

export function Icon({ name, size = 20, className, title }: { name: string; size?: number; className?: string; title?: string }) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, '');
  const own = OWN[name];
  return (
    <svg className={className ? `icon ${className}` : 'icon'} width={size} height={size} viewBox="0 0 20 20" aria-hidden={title ? undefined : true} role={title ? 'img' : undefined}>
      {title && <title>{title}</title>}
      {own ? own(id) : LUCIDE[name] ? lucide(name) : <rect x="4" y="4" width="12" height="12" rx="1.5" {...S} />}
    </svg>
  );
}
