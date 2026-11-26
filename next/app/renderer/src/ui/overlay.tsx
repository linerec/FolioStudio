// What InDesign draws over the page in Normal view: bleed, margin and column guides, ruler
// guides, frame edges in the layer colour, the selection with its handles, text frame ports.
// Also hit testing for the selection tools (points from the page's top-left).
import type { Box, Corner, Matrix, DocumentInfo, ItemInfo, PageItems, PageSide, RGB } from '../inspect';
import { apply, corners, distance, inside, physicalMargins, unionBox } from '../inspect';
import { markPlacement, LIVE_SIZE, ANCHOR_BOX, ANCHOR_SQUARE, type ViewMarks } from '../features/view/marks';
import { diamondPlacement, DIAMOND_HALF } from '../features/view/livecorners';
import type { LinkStatus } from '../engine';
import { t as tr } from '../i18n';
import { columnLines } from './snap';
import { BracketMarks } from '../features/tools/textpath-handles';
import type { BracketView } from '../features/tools/useTextPathBrackets';
import { type SmartDraw, SMART_COLOR, SMART_PAGE_COLOR, SMART_PX, hasDraw } from './smartguides';

export type SelEntry = { page: number; id: string; content: boolean };

export type Hit = { page: number; top: ItemInfo; path: ItemInfo[]; leaf: ItemInfo };

const css = (c: RGB | null | undefined, a = 1) => (c ? `rgba(${c[0]},${c[1]},${c[2]},${a})` : `rgba(79,153,255,${a})`);
export const CONTENT_COLOR: RGB = [196, 120, 56];

export type LayerLook = Map<string, { color: RGB; visible: boolean; locked: boolean; guides: boolean }>;

export function layerLook(info: DocumentInfo | null): LayerLook {
  return new Map((info?.layers ?? []).map((l) => [l.id, { color: l.color, visible: l.visible, locked: l.locked, guides: l.showGuides !== false }]));
}

function shape(item: ItemInfo): { poly: [number, number][]; closed: boolean } {
  const poly = item.outline?.[0]?.length ? item.outline[0] : corners(item.transform, item.inner);
  return { poly, closed: item.kind !== 'line' };
}

function hitLeaf(item: ItemInfo, pt: [number, number], tol: number, strokeOnly = false): boolean {
  const { poly, closed } = shape(item);
  // strokeOnly (Type on a Path tool): only the path itself counts, never the inside of a filled shape or frame
  const solid = !strokeOnly && (item.kind === 'textFrame' || item.content !== null || item.children.length > 0 || item.fill.kind !== 'none');
  if (solid && closed && inside(pt, poly)) return true;
  const extra = item.stroke.kind !== 'none' ? item.stroke.weight / 2 : 0;
  if (distance(pt, poly, closed) <= tol + extra) return true;
  // other subpaths (compound shapes)
  for (const sub of item.outline?.slice(1) ?? []) {
    if ((solid && inside(pt, sub)) || distance(pt, sub, true) <= tol + extra) return true;
  }
  return false;
}

/** The deepest item under the point: [top-level item, …, leaf]. */
function hitPath(item: ItemInfo, pt: [number, number], tol: number, strokeOnly = false): ItemInfo[] | null {
  if (!item.visible) return null;
  if (item.kind === 'group') {
    for (let i = item.children.length - 1; i >= 0; i--) {
      const p = hitPath(item.children[i], pt, tol, strokeOnly);
      if (p) return [item, ...p];
    }
    return null;
  }
  return hitLeaf(item, pt, tol, strokeOnly) ? [item] : null;
}

/** Top-most selectable item under `pt` on a page; master items and locked layers are skipped (`parent`: the parent's items only). */
export function hitTest(items: PageItems, page: number, pt: [number, number], tol: number, layers: LayerLook, parent = false, strokeOnly = false): Hit | null {
  const list = [...(parent ? items.masterItems : items.items)].sort((a, b) => b.z - a.z);
  for (const it of list) {
    const l = it.layer ? layers.get(it.layer) : undefined;
    if (l && (!l.visible || l.locked)) continue;
    if (it.locked) continue;
    const path = hitPath(it, pt, tol, strokeOnly);
    if (path) return { page, top: it, path, leaf: path[path.length - 1] };
  }
  return null;
}

/** Every top-level object at a point, front to back (Ctrl+click selects the one behind the selected one: tools-c1). */
export function hitTestAll(items: PageItems, page: number, pt: [number, number], tol: number, layers: LayerLook): Hit[] {
  const out: Hit[] = [];
  for (const it of [...items.items].sort((a, b) => b.z - a.z)) {
    const l = it.layer ? layers.get(it.layer) : undefined;
    if ((l && (!l.visible || l.locked)) || it.locked) continue;
    const path = hitPath(it, pt, tol, false);
    if (path) out.push({ page, top: it, path, leaf: path[path.length - 1] });
  }
  return out;
}

/** Items whose bounds meet a rectangle (marquee). */
export function itemsInRect(items: PageItems, r: [number, number, number, number], layers: LayerLook): ItemInfo[] {
  return items.items.filter((it) => {
    const l = it.layer ? layers.get(it.layer) : undefined;
    if (it.locked || !it.visible || (l && (!l.visible || l.locked))) return false;
    const b = it.bounds;
    return b[0] <= r[2] && b[2] >= r[0] && b[1] <= r[3] && b[3] >= r[1];
  });
}

export function findItem(items: PageItems | null | undefined, id: string): { item: ItemInfo; path: ItemInfo[] } | null {
  if (!items) return null;
  const walk = (list: ItemInfo[], up: ItemInfo[]): { item: ItemInfo; path: ItemInfo[] } | null => {
    for (const it of list) {
      if (it.id === id) return { item: it, path: [...up, it] };
      const r = walk(it.children, [...up, it]);
      if (r) return r;
    }
    return null;
  };
  return walk(items.items, []) ?? walk(items.masterItems, []);
}

type OverlayProps = {
  contentGrabber?: boolean;
  items: PageItems | null | undefined;
  scale: number;
  layers: LayerLook;
  side: PageSide | undefined;
  /** Document bleed: top, bottom, inside, outside (points). */
  bleed: { top: number; bottom: number; inside: number; outside: number } | null;
  guides: boolean;
  edges: boolean;
  selection: SelEntry[];
  page: number;
  hover: string | null;
  showContentGrabber?: boolean;
  /** View ▸ Extras ▸ Live Corners / Anchored Object Control, for the tool in use (absent: neither is drawn). */
  marks?: ViewMarks | null;
  /** The live corner of this frame is in edit mode: its four diamonds (their corners as they are now or while one is dragged) replace the yellow square, the blue square and the corner handles. */
  liveEdit?: { id: string; corners: Corner[] } | null;
  /** The bracket handles of the selected text path on this page (Selection, Direct Selection and Type on a Path tool). */
  brackets?: BracketView | null;
  /** View ▸ Extras ▸ Show Link Badge: the links by `page:file name` as the frame's graphic shows it (absent: no badges). */
  linkBadges?: Map<string, { owner: string; status: LinkStatus }> | null;
  marquee: [number, number, number, number] | null;
  /** The Measure tool's line [x0, y0, x1, y1], and with the protractor its second line from x0, y0 to [x2, y2]. */
  measure: number[] | null;
  /** Page tool: this page is the selected page. */
  pageSelected: boolean;
  /** Spread guides run out past the page on these sides. */
  outer: { left: boolean; right: boolean };
  size: [number, number];
  /** While dragging: where the selection would go (moved), or a frame's new box. */
  ghost?: Ghost | null;
  /** Smart Guides during the drag: alignment lines, equal-gap arrows, equal-size brackets (ui/smartguides.ts). */
  smartDraw?: SmartDraw | null;
  /** The colour of the object lines, arrows and brackets of the Smart Guides (CSS; the preference's colour, ui/smartcolor.ts). */
  smartColor?: string;
  /** The Rotate tool's reference point (page coordinates). */
  pivot?: { x: number; y: number } | null;
  /** The selected ruler guides (ids joined by commas): drawn in their layer's colour, as InDesign does. */
  guide?: string | null;
  /** View ▸ Grids & Guides: the baseline grid and the document grid, when shown. */
  grids?: import('../inspect').ViewInfo | null;
};

/** Link badges: a small mark inside the top left corner of every frame that holds a linked graphic (InDesign's View ▸ Extras ▸ Show Link Badge). */
export const BADGE = 14, BADGE_INSET = 2;
export type BadgeAt = { owner: string; status: LinkStatus; x: number; y: number; id: string };
export type BadgeLinks = Map<string, { owner: string; status: LinkStatus }>;
export function linkBadgeList(items: PageItems | null | undefined, status: BadgeLinks | null | undefined, page: number): BadgeAt[] {
  const out: BadgeAt[] = [];
  const walk = (list: ItemInfo[]) => {
    for (const it of list) {
      const name = it.content?.link;
      const hit = name ? status?.get(`${page}:${name}`) : undefined;
      if (hit && hit.status !== 'embedded') { const [x, y] = corners(it.transform, it.inner)[0]; out.push({ owner: hit.owner, status: hit.status, x, y, id: it.id }); }
      if (it.children?.length) walk(it.children);
    }
  };
  if (status && items) walk(items.items);
  return out;
}
/** The badge under a point of the page, if any (page points; `scale` = screen pixels per point). */
export function linkBadgeAt(items: PageItems | null | undefined, status: BadgeLinks | null | undefined, page: number, pt: [number, number], scale: number): BadgeAt | null {
  return linkBadgeList(items, status, page).find((b) => {
    const dx = (pt[0] - b.x) * scale, dy = (pt[1] - b.y) * scale;
    return dx >= BADGE_INSET && dx <= BADGE_INSET + BADGE && dy >= BADGE_INSET && dy <= BADGE_INSET + BADGE;
  }) ?? null;
}
const BADGE_LOOK: Record<string, { color: string; label: () => string }> = {
  found: { color: '#2f7de1', label: () => tr('연결됨') },
  modified: { color: '#e0a800', label: () => tr('수정됨') },
  missing: { color: '#d33', label: () => tr('연결이 끊김') },
  ambiguous: { color: '#d97a00', label: () => tr('같은 이름이 여럿') },
};
function LinkBadge({ b, s }: { b: BadgeAt; s: number }) {
  const look = BADGE_LOOK[b.status] ?? BADGE_LOOK.found;
  const half = BADGE / 2;
  return <g className={`link-badge ${b.status}`} data-link={b.owner} transform={`translate(${b.x * s + BADGE_INSET} ${b.y * s + BADGE_INSET})`}>
    <title>{look.label()}</title>
    {b.status === 'modified' ? <path d={`M${half} 0 L${BADGE} ${BADGE} L0 ${BADGE} Z`} fill={look.color} stroke="#fff" strokeWidth={1} />
      : <circle cx={half} cy={half} r={half} fill={look.color} stroke="#fff" strokeWidth={1} />}
    {b.status === 'found'
      ? <path d="M4.2 7 h5.6 M5.4 5.2 a2 2 0 0 0 0 3.6 M8.6 5.2 a2 2 0 0 1 0 3.6" fill="none" stroke="#fff" strokeWidth={1.2} strokeLinecap="round" />
      : <text x={half} y={BADGE - 3} textAnchor="middle" fontSize={9} fontWeight={700} fill="#fff">{b.status === 'modified' ? '!' : '?'}</text>}
  </g>;
}

/** The live corner (a yellow square on the right edge) and the anchored object control (a square on the top edge; the anchor symbol for an object set in text) of a single selected object. */
function viewMarkShapes(it: ItemInfo, s: number, marks: ViewMarks, color: RGB | null, liveEdit?: { id: string; corners: Corner[] } | null): React.ReactElement[] {
  const m = markPlacement(it, s, marks), stroke = css(color), out: React.ReactElement[] = [];
  if (liveEdit?.id === it.id) {
    const q = DIAMOND_HALF;
    return diamondPlacement(it, s, liveEdit.corners).map(([x, y], k) => (
      <polygon key={`diamond${it.id}${k}`} className="live-diamond" data-item={it.id} data-corner={k} fill="#ffe500" stroke={stroke} strokeWidth={1}
        points={`${Math.round(x)},${Math.round(y) - q} ${Math.round(x) + q},${Math.round(y)} ${Math.round(x)},${Math.round(y) + q} ${Math.round(x) - q},${Math.round(y)}`} />
    ));
  }
  if (m.live) {
    const h = LIVE_SIZE / 2;
    out.push(<g key={`live${it.id}`} className="live-corner" data-item={it.id} transform={`translate(${Math.round(m.live.x)} ${Math.round(m.live.y)})`}>
      <rect x={-h} y={-h} width={LIVE_SIZE} height={LIVE_SIZE} fill="#ffe500" stroke={stroke} strokeWidth={1} />
    </g>);
  }
  if (m.anchor) {
    // the widget sits on whole screen pixels like InDesign's: its top-left pixel is the rounded centre minus half the size (a 19 x 19 box with a 1 px
    // border, a 9 x 8 solid square), the group is placed at the widget's centre
    const a = m.anchor;
    const bw = m.anchored ? ANCHOR_BOX : ANCHOR_SQUARE, bh = m.anchored ? ANCHOR_BOX : ANCHOR_SQUARE - 1;
    const gx = Math.round(a.x - bw / 2) + bw / 2, gy = Math.round(a.y - bh / 2) + bh / 2;
    out.push(m.anchored
      ? <g key={`anchor${it.id}`} className="anchor-control anchored" data-item={it.id} transform={`translate(${gx} ${gy})`}>
        <rect x={-(bw - 1) / 2} y={-(bh - 1) / 2} width={bw - 1} height={bh - 1} fill="#fff" stroke={stroke} strokeWidth={1} />
        <path d="M0 -5 v9 M-2.5 -2 h5 M-5 1 a5 5 0 0 0 10 0 M-5 1 l-1.2 -1.6 M5 1 l1.2 -1.6" fill="none" stroke="rgba(79,153,255,0.45)" strokeWidth={1.4} strokeLinecap="round" />
        <circle cx={0} cy={-6} r={1.6} fill="none" stroke="rgba(79,153,255,0.45)" strokeWidth={1.2} />
      </g>
      : <g key={`anchor${it.id}`} className="anchor-control" data-item={it.id} transform={`translate(${gx} ${gy})`}>
        <rect x={-bw / 2} y={-bh / 2} width={bw} height={bh} fill={stroke} />
      </g>);
  }
  return out;
}

/** A drag in progress, drawn as a dashed outline. */
export type Ghost =
  /** `copies`: the offsets of a grid of copies (Alt-drag with the arrows), drawn as well. */
  | { kind: 'move'; ids: string[]; dx: number; dy: number; copies?: [number, number][] }
  | { kind: 'matrix'; ids: string[]; matrix: Matrix }
  | { kind: 'gradient'; from: [number, number]; to: [number, number] }
  | { kind: 'line'; from: [number, number]; to: [number, number] }
  | { kind: 'box'; id: string; transform: [number, number, number, number, number, number]; rect: [number, number, number, number] };

/** Single/group inner handles, or the selection's common box in page axes. */
export function handleAt(items: PageItems | null | undefined, sel: SelEntry[], page: number, pt: [number, number], tol: number): { id: string; ids: string[]; fx: number; fy: number; box: Box; transform: Matrix } | null {
  if (!sel.length || sel.some(s=>s.content) || !items) return null;
  const all = sel.map(s=>findItem(items,s.id));
  if (all.some(f=>!f || f.item.locked)) return null;
  const f = all[0]!;
  if (sel.length === 1 && (sel[0].page !== page || f.item.kind === 'line')) return null;
  const b: Box = sel.length === 1 ? f.item.inner : unionBox(all.map(f=>f!.item.bounds))!;
  const m: Matrix = sel.length === 1 ? f.item.transform : [1,0,0,1,0,0];
  for (const fy of [0, 0.5, 1]) {
    for (const fx of [0, 0.5, 1]) {
      if (fx === 0.5 && fy === 0.5) continue;
      const [x, y] = apply(m, b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
      if (Math.abs(x - pt[0]) <= tol && Math.abs(y - pt[1]) <= tol) return { id: f.item.id, ids:sel.map(s=>s.id), fx, fy, box:b, transform:m };
    }
  }
  return null;
}

/** The selection when the pointer is just outside one of its four corner handles (not on it, not inside the frame): a drag from there
 *  turns it (the Selection tool's rotate zone, `zone` points wide; InDesign has none at the side handles). The same guards as `handleAt`. */
export function rotateAt(items: PageItems | null | undefined, sel: SelEntry[], page: number, pt: [number, number], zone: number): { ids: string[] } | null {
  if (!sel.length || sel.some(s => s.content) || !items) return null;
  const all = sel.map(s => findItem(items, s.id));
  if (all.some(f => !f || f.item.locked)) return null;
  const f = all[0]!;
  if (sel.length === 1 && (sel[0].page !== page || f.item.kind === 'line')) return null;
  const b: Box = sel.length === 1 ? f.item.inner : unionBox(all.map(f => f!.item.bounds))!;
  const m: Matrix = sel.length === 1 ? f.item.transform : [1, 0, 0, 1, 0, 0];
  if (inside(pt, corners(m, b))) return null;
  for (const fy of [0, 1]) {
    for (const fx of [0, 1]) {
      const [x, y] = apply(m, b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
      if (Math.hypot(x - pt[0], y - pt[1]) <= zone) return { ids: sel.map(s => s.id) };
    }
  }
  return null;
}

const px = (v: number, s: number) => Math.round(v * s) + 0.5;
const pts = (poly: [number, number][], s: number) => poly.map(([x, y]) => `${(x * s).toFixed(1)},${(y * s).toFixed(1)}`).join(' ');

export function PageOverlay(p: OverlayProps) {
  const s = p.scale;
  const [w, h] = p.size;
  const items = p.items;
  const out: React.ReactNode[] = [];

  // the document grid (every `horizontal` / `vertical` pt, with subdivisions) and the baseline grid (from its start,
  // below the zoom threshold not shown); a right-hand page continues the left page's grid
  const v = p.grids;
  const grid: React.ReactNode[] = [];
  if (v?.documentGridShown) {
    const x0 = p.outer.left ? 0 : w;
    const g = v.grid, c = css(g.color, 0.7), minor = css(g.color, 0.35);
    const lines = (len: number, step: number, sub: number, from: number, horizontal: boolean) => {
      const n = Math.max(1, Math.round(sub));
      const gap = step / n;
      if (!(gap > 0) || gap * s < 3) return;
      for (let k = Math.ceil(from / gap - 1e-9); k * gap - from <= len + 1e-9; k++) {
        const at = k * gap - from;
        const major = k % n === 0;
        grid.push(horizontal
          ? <line key={`dh${k}`} className="g-grid" stroke={major ? c : minor} x1={0} x2={Math.round(w * s)} y1={px(at, s)} y2={px(at, s)} />
          : <line key={`dv${k}`} className="g-grid" stroke={major ? c : minor} y1={0} y2={Math.round(h * s)} x1={px(at, s)} x2={px(at, s)} />);
      }
    };
    lines(h, g.horizontal, g.horizontalSub, 0, true);
    lines(w, g.vertical, g.verticalSub, x0, false);
  }
  if (v?.baselineGridShown && s * 100 >= v.baseline.threshold && v.baseline.division > 0) {
    const b = v.baseline;
    const top = b.relativeToMargin && items ? physicalMargins(items.margins, items.side ?? p.side)[0] : 0;
    const c = css(b.color, 0.9);
    for (let y = top + b.start, k = 0; y <= h + 1e-6 && k < 4000; y += b.division, k++) {
      grid.push(<line key={`b${k}`} className="g-baseline" stroke={c} x1={0} x2={Math.round(w * s)} y1={px(y, s)} y2={px(y, s)} />);
    }
  }
  if (grid.length && v) {
    // "Grids in Back": the grids lie behind the page's filled shapes and pictures, so those cut them off
    const id = `grid-mask-${p.page}`;
    const holes: [number, number][][] = [];
    const cut = (it: ItemInfo) => {
      const l = it.layer ? p.layers.get(it.layer) : undefined;
      if (!it.visible || (l && !l.visible)) return;
      if (it.kind === 'group') it.children.forEach(cut);
      else if (it.fill.kind !== 'none' || it.content) holes.push(shape(it).poly);
    };
    if (v.gridsInBack && items) [...items.masterItems, ...items.items].forEach(cut);
    out.push(
      <g key="grids" mask={holes.length ? `url(#${id})` : undefined}>
        {holes.length > 0 && (
          <mask id={id} maskUnits="userSpaceOnUse" x={0} y={0} width={Math.ceil(w * s) + 2} height={Math.ceil(h * s) + 2}>
            <rect x={0} y={0} width={Math.ceil(w * s) + 2} height={Math.ceil(h * s) + 2} fill="#fff" />
            {holes.map((poly, i) => <polygon key={i} points={pts(poly, s)} fill="#000" />)}
          </mask>
        )}
        {grid}
      </g>,
    );
  }

  // bleed guide
  if (p.guides && p.bleed && (p.bleed.top || p.bleed.bottom || p.bleed.inside || p.bleed.outside)) {
    const left = p.side === 'left' ? p.bleed.outside : p.side === 'right' ? p.bleed.inside : p.bleed.inside;
    const right = p.side === 'left' ? p.bleed.inside : p.bleed.outside;
    const l = p.side === 'right' && !p.outer.left ? 0 : left;
    const r = p.side === 'left' && !p.outer.right ? 0 : right;
    out.push(<rect key="bleed" className="g-bleed" style={p.grids ? { stroke: css(p.grids.pasteboard.bleedColor) } : undefined} x={px(-l, s)} y={px(-p.bleed.top, s)} width={Math.round((w + l + r) * s)} height={Math.round((h + p.bleed.top + p.bleed.bottom) * s)} />);
  }

  if (p.guides && items) {
    const [mt, ml, mb, mr] = physicalMargins(items.margins, items.side ?? p.side);
    if (mt || ml || mb || mr) {
      out.push(<rect key="margins" className="g-margin" x={px(ml, s)} y={px(mt, s)} width={Math.round((w - ml - mr) * s)} height={Math.round((h - mt - mb) * s)} />);
    }
    columnLines(items, w, [mt, ml, mb, mr]).forEach((x, i) => out.push(<line key={`c${i}`} className="g-column" x1={px(x, s)} x2={px(x, s)} y1={px(mt, s)} y2={px(h - mb, s)} />));
    // Preferences > Guides & Pasteboard: ruler guides show from this zoom (percent) on
    const chosenGuides = new Set(p.guide ? p.guide.split(',') : []);
    items.guides.forEach((g, i) => {
      const l = g.layer ? p.layers.get(g.layer) : undefined;
      if ((l && (!l.visible || !l.guides)) || s * 100 < g.viewThreshold) return;
      const stroke = css(chosenGuides.has(g.id) ? l?.color ?? g.color : g.color, 0.95);
      if (g.orientation === 'horizontal') {
        const x1 = !g.fitToPage && p.outer.left ? -400 : 0;
        const x2 = !g.fitToPage && p.outer.right ? w + 400 / s : w;
        out.push(<line key={`g${i}`} className="g-ruler" data-guide={g.id} data-parent={g.master || undefined} data-chosen={chosenGuides.has(g.id) || undefined} stroke={stroke} x1={x1 < 0 ? x1 : px(x1, s)} x2={px(x2, s)} y1={px(g.location, s)} y2={px(g.location, s)} />);
      } else {
        out.push(<line key={`g${i}`} className="g-ruler" data-guide={g.id} data-parent={g.master || undefined} data-chosen={chosenGuides.has(g.id) || undefined} stroke={stroke} y1={0} y2={Math.round(h * s)} x1={px(g.location, s)} x2={px(g.location, s)} />);
      }
    });
  }

  // frame edges
  if (p.edges && items) {
    const edge = (it: ItemInfo, master: boolean, key: string) => {
      const l = it.layer ? p.layers.get(it.layer) : undefined;
      if (!it.visible || (l && !l.visible)) return;
      if (it.kind === 'group') {
        it.children.forEach((c, i) => edge(c, master, `${key}.${i}`));
        return;
      }
      const { poly, closed } = shape(it);
      const color = css(l?.color, master ? 0.75 : 0.85);
      const common = { key, stroke: color, className: master ? 'edge master' : 'edge', points: pts(poly, s) };
      out.push(closed ? <polygon {...common} /> : <polyline {...common} />);
      const empty = closed && it.kind !== 'textFrame' && !it.content && !it.children.length && it.fill.kind === 'none' && it.stroke.kind === 'none';
      if (empty && poly.length === 4) {
        out.push(<line key={`${key}x1`} className="edge" stroke={color} x1={poly[0][0] * s} y1={poly[0][1] * s} x2={poly[2][0] * s} y2={poly[2][1] * s} />);
        out.push(<line key={`${key}x2`} className="edge" stroke={color} x1={poly[1][0] * s} y1={poly[1][1] * s} x2={poly[3][0] * s} y2={poly[3][1] * s} />);
      }
    };
    items.masterItems.forEach((it, i) => edge(it, true, `m${i}`));
    items.items.forEach((it, i) => edge(it, false, `i${i}`));
  }

  // hover highlight
  if (items && p.hover && !p.selection.some((e) => e.id === p.hover)) {
    const f = findItem(items, p.hover);
    if (f) {
      const l = f.item.layer ? p.layers.get(f.item.layer) : undefined;
      const { poly, closed } = shape(f.item);
      const common = { key: 'hover', className: 'hover', stroke: css(l?.color), points: pts(poly, s) };
      out.push(closed ? <polygon {...common} /> : <polyline {...common} />);
    }
  }

  // selection
  const mine = p.selection.filter((e) => e.page === p.page);
  if (items && mine.length) {
    const many = p.selection.length > 1;
    for (const e of mine) {
      const f = findItem(items, e.id);
      if (!f) continue;
      const it = f.item;
      const l = (f.path[0].layer ? p.layers.get(f.path[0].layer) : undefined)?.color ?? null;
      if (e.content && it.content) {
        out.push(<Frame key={`sel${e.id}c`} poly={corners(it.content.transform, it.content.inner)} s={s} color={CONTENT_COLOR} handles={!many} />);
        // the frame stays outlined
        out.push(<polygon key={`sel${e.id}f`} className="sel-edge" stroke={css(l)} points={pts(shape(it).poly, s)} />);
        continue;
      }
      const poly = it.kind === 'group' ? corners(it.transform, it.inner) : shape(it).poly;
      if (it.kind === 'line') {
        out.push(<polyline key={`sel${e.id}`} className="sel-edge" stroke={css(l)} points={pts(poly, s)} />);
        poly.forEach(([x, y], i) => out.push(<rect key={`sel${e.id}h${i}`} className="handle" stroke={css(l)} x={x * s - 3.5} y={y * s - 3.5} width={7} height={7} />));
        if (!many && p.marks) out.push(...viewMarkShapes(it, s, p.marks, l));
        if (!many && p.brackets?.id === it.id) out.push(<BracketMarks key={`tp${e.id}`} it={it} view={p.brackets} s={s} color={css(l)} />);
        continue;
      }
      const box = corners(it.transform, it.inner);
      const liveOn = !many && p.liveEdit?.id === it.id;
      out.push(<Frame key={`sel${e.id}`} poly={box} s={s} color={l} handles={!many} cornerHandles={!liveOn} dashed={it.kind === 'group'} />);
      if (p.contentGrabber !== false && p.showContentGrabber !== false && !many && it.content) out.push(<Grabber key={`grab${e.id}`} it={it} s={s} />);
      if (it.text) out.push(<Ports key={`ports${e.id}`} box={box} s={s} color={l} text={it.text} />);
      if (!many && p.marks) out.push(...viewMarkShapes(it, s, p.marks, l, p.liveEdit));
      if (!many && p.brackets?.id === it.id) out.push(<BracketMarks key={`tp${e.id}`} it={it} view={p.brackets} s={s} color={css(l)} />);
    }
  }

  // several selected: one bounding box with handles around them (per page)
  if (items && p.selection.length > 1) {
    const boxes = p.selection.map((e) => findItem(items, e.id)?.item.bounds).filter((b): b is [number, number, number, number] => !!b);
    if (boxes.length === p.selection.length) {
      const u: [number, number, number, number] = [Math.min(...boxes.map((b) => b[0])), Math.min(...boxes.map((b) => b[1])), Math.max(...boxes.map((b) => b[2])), Math.max(...boxes.map((b) => b[3]))];
      out.push(<Frame key="union" poly={[[u[0], u[1]], [u[2], u[1]], [u[2], u[3]], [u[0], u[3]]]} s={s} color={null} handles />);
    }
  }

  // a drag in progress
  const g = p.ghost;
  if (g?.kind === 'line') out.push(<line key="ghostline" className="ghost" x1={g.from[0] * s} y1={g.from[1] * s} x2={g.to[0] * s} y2={g.to[1] * s} />);
  if (g?.kind === 'gradient') out.push(<g key="gradient" className="gradient-preview">
    <line className="ghost" x1={g.from[0] * s} y1={g.from[1] * s} x2={g.to[0] * s} y2={g.to[1] * s} />
    <circle className="measure-end" cx={g.from[0] * s} cy={g.from[1] * s} r={4} />
    <circle className="measure-end" cx={g.to[0] * s} cy={g.to[1] * s} r={3} />
  </g>);
  if (items && g?.kind === 'move') {
    for (const id of g.ids) {
      const f = findItem(items, id);
      if (!f) continue;
      const { poly, closed } = f.item.kind === 'group' ? { poly: corners(f.item.transform, f.item.inner), closed: true } : shape(f.item);
      for (const [k, [dx, dy]] of (g.copies ?? [[g.dx, g.dy]]).entries()) {
        const moved = poly.map(([x, y]) => [x + dx, y + dy] as [number, number]);
        const common = { key: `ghost${id}-${k}`, className: 'ghost', points: pts(moved, s), ...(g.copies ? { 'data-grid-copy': k } : {}) };
        out.push(closed ? <polygon {...common} /> : <polyline {...common} />);
      }
    }
  }
  if (items && g?.kind === 'matrix') {
    const turn = (pt: [number, number]) => apply(g.matrix, pt[0], pt[1]);
    for (const id of g.ids) {
      const f = findItem(items, id);
      if (!f) continue;
      const { poly, closed } = f.item.kind === 'group' ? { poly: corners(f.item.transform, f.item.inner), closed: true } : shape(f.item);
      const common = { key: `turn${id}`, className: 'ghost', points: pts(poly.map(turn), s) };
      out.push(closed ? <polygon {...common} /> : <polyline {...common} />);
    }
  }
  if (p.pivot) {
    const [x, y] = [p.pivot.x * s, p.pivot.y * s];
    out.push(<g key="pivot" className="pivot"><circle cx={x} cy={y} r={5} /><line x1={x - 9} y1={y} x2={x + 9} y2={y} /><line x1={x} y1={y - 9} x2={x} y2={y + 9} /></g>);
  }
  if (g?.kind === 'box') out.push(<polygon key="ghostbox" className="ghost" points={pts(corners(g.transform, g.rect), s)} />);
  if (p.smartDraw && hasDraw(p.smartDraw)) out.push(<SmartLayer key="smart" d={p.smartDraw} s={s} color={p.smartColor ?? SMART_COLOR} />);

  if (p.pageSelected) out.push(<rect key="pagesel" className="page-sel" x={0.5} y={0.5} width={Math.round(w * s) - 1} height={Math.round(h * s) - 1} />);

  if (p.marquee) {
    const [x0, y0, x1, y1] = p.marquee;
    out.push(<rect key="marquee" className="marquee" x={Math.min(x0, x1) * s} y={Math.min(y0, y1) * s} width={Math.abs(x1 - x0) * s} height={Math.abs(y1 - y0) * s} />);
  }
  if (p.measure) {
    const [x0, y0, x1, y1] = p.measure;
    out.push(<line key="measure" className="measure" x1={x0 * s} y1={y0 * s} x2={x1 * s} y2={y1 * s} />);
    out.push(<circle key="m0" className="measure-end" cx={x0 * s} cy={y0 * s} r={3} />);
    out.push(<circle key="m1" className="measure-end" cx={x1 * s} cy={y1 * s} r={3} />);
    if (p.measure.length >= 6) {
      const [x2, y2] = p.measure.slice(4);
      out.push(<line key="measure2" className="measure measure-leg" x1={x0 * s} y1={y0 * s} x2={x2 * s} y2={y2 * s} />);
      out.push(<circle key="m2" className="measure-end" cx={x2 * s} cy={y2 * s} r={3} />);
    }
  }

  if (p.linkBadges) for (const b of linkBadgeList(p.items, p.linkBadges, p.page)) out.push(<LinkBadge key={`badge${b.id}`} b={b} s={s} />);
  return (
    <svg className="page-overlay" width={Math.round(w * s)} height={Math.round(h * s)}>
      {out}
    </svg>
  );
}

/** Smart Guides: 1 device pixel lines in InDesign's smart guide colour; offsets measured in InDesign's device pixels (125 % scaling, SMART_PX). */
function SmartLayer({ d, s, color: lineColor }: { d: SmartDraw; s: number; color: string }) {
  const u = 1 / SMART_PX.scale, sw = 1 / (window.devicePixelRatio || 1);
  const o: React.ReactNode[] = [];
  const L = (k: string, x1: number, y1: number, x2: number, y2: number, color = lineColor) =>
    o.push(<line key={k} className="smart-line" x1={x1} y1={y1} x2={x2} y2={y2} stroke={color} strokeWidth={sw} />);
  d.lines.forEach((l, i) => (l.axis === 'x'
    ? L(`l${i}`, l.at * s, l.from * s, l.at * s, l.to * s, l.kind === 'page' ? SMART_PAGE_COLOR : lineColor)
    : L(`l${i}`, l.from * s, l.at * s, l.to * s, l.at * s, l.kind === 'page' ? SMART_PAGE_COLOR : lineColor)));
  d.gaps.forEach((g, i) => {
    const h = g.axis === 'h', a = g.from * s, b = g.to * s, c = g.at * s + SMART_PX.gapOffset * u, t = SMART_PX.gapTick * u / 2, [hl, hh] = SMART_PX.gapHead;
    const P = (x: number, y: number): [number, number] => (h ? [x, y] : [y, x]);
    const seg = (k: string, p1: [number, number], p2: [number, number]) => L(k, ...P(...p1), ...P(...p2));
    // each end mark starts 4 px past the bottom (right) edge of its own object and ends 7 px past the arrow (measured: a shorter object has a longer mark)
    const top = (edge: number | undefined) => (edge === undefined ? c - t : Math.min(c - t, edge * s + (SMART_PX.gapOffset - SMART_PX.gapTick / 2) * u));
    seg(`g${i}a`, [a, c], [b, c]); seg(`g${i}t0`, [a, top(g.ends?.[0])], [a, c + t]); seg(`g${i}t1`, [b, top(g.ends?.[1])], [b, c + t]);
    seg(`g${i}h0`, [a + hl * u, c - hh * u], [a, c]); seg(`g${i}h1`, [a + hl * u, c + hh * u], [a, c]);
    seg(`g${i}h2`, [b - hl * u, c - hh * u], [b, c]); seg(`g${i}h3`, [b - hl * u, c + hh * u], [b, c]);
  });
  d.sizes.forEach((z, i) => {
    if (z.seg && z.out) {
      // a turned target: the bracket along its own side, 10 px away from it, the caps square to it
      const off = SMART_PX.sizeOffset * u, cap = SMART_PX.sizeCap * u / 2, [nx, ny] = z.out;
      const q = (x: number, y: number, k: number): [number, number] => [x * s + nx * (off + k), y * s + ny * (off + k)];
      L(`z${i}a`, ...q(z.seg[0], z.seg[1], 0), ...q(z.seg[2], z.seg[3], 0));
      L(`z${i}t0`, ...q(z.seg[0], z.seg[1], -cap), ...q(z.seg[0], z.seg[1], cap));
      L(`z${i}t1`, ...q(z.seg[2], z.seg[3], -cap), ...q(z.seg[2], z.seg[3], cap));
      return;
    }
    const h = z.axis === 'w', a = z.from * s, b = z.to * s, c = z.at * s + SMART_PX.sizeOffset * u, t = SMART_PX.sizeCap * u / 2;
    const P = (x: number, y: number): [number, number] => (h ? [x, y] : [y, x]);
    const seg = (k: string, p1: [number, number], p2: [number, number]) => L(k, ...P(...p1), ...P(...p2));
    seg(`z${i}a`, [a, c], [b, c]); seg(`z${i}t0`, [a, c - t], [a, c + t]); seg(`z${i}t1`, [b, c - t], [b, c + t]);
  });
  return <g className="smart-guides">{o}</g>;
}

/** A selected frame: outline, eight handles and the centre point. */
function Frame({ poly, s, color, handles, dashed, cornerHandles = true }: { poly: [number, number][]; s: number; color: RGB | null; handles: boolean; dashed?: boolean; cornerHandles?: boolean }) {
  const stroke = css(color);
  const p = poly.map(([x, y]) => [x * s, y * s] as [number, number]);
  const mid = (a: [number, number], b: [number, number]): [number, number] => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  const all = handles ? [p[0], mid(p[0], p[1]), p[1], mid(p[1], p[2]), p[2], mid(p[2], p[3]), p[3], mid(p[3], p[0])] : [];
  // the live corner's diamonds stand where the corner handles were (the side handles stay)
  const hs = cornerHandles ? all : all.filter((_, i) => i % 2 === 1);
  const c = mid(p[0], p[2]);
  return (
    <g>
      <polygon className={dashed ? 'sel-edge dashed' : 'sel-edge'} stroke={stroke} points={p.map((q) => q.join(',')).join(' ')} />
      {hs.map(([x, y], i) => <rect key={i} className="handle" stroke={stroke} x={Math.round(x) - 3.5} y={Math.round(y) - 3.5} width={7} height={7} />)}
      {handles && <rect className="center" fill={stroke} x={Math.round(c[0]) - 2} y={Math.round(c[1]) - 2} width={4} height={4} />}
    </g>
  );
}

/** The content grabber (the ring in a frame with a placed graphic). */
function Grabber({ it, s }: { it: ItemInfo; s: number }) {
  const b = it.bounds;
  const cx = ((b[0] + b[2]) / 2) * s, cy = ((b[1] + b[3]) / 2) * s;
  return (
    <g className="grabber">
      <circle cx={cx} cy={cy} r={9} />
      <circle cx={cx} cy={cy} r={4.5} />
    </g>
  );
}

/** The in or out port of a selected text frame under `pt` (page points); `s` = px per point.
 * Same geometry as `Ports` draws. */
export function portAt(items: PageItems | null | undefined, sel: SelEntry[], page: number, pt: [number, number], s: number): { item: ItemInfo; port: 'in' | 'out' } | null {
  if (!items || sel.length !== 1 || sel[0].page !== page || sel[0].content) return null;
  const f = findItem(items, sel[0].id);
  if (!f || !f.item.text || f.item.locked) return null;
  const [tl, , br] = corners(f.item.transform, f.item.inner);
  const off = Math.min(18, ((br[1] - tl[1]) * s) / 3) / s, tol = 6 / s;
  const near = (x: number, y: number) => Math.abs(pt[0] - x) <= tol && Math.abs(pt[1] - y) <= tol;
  if (near(br[0], br[1] - off)) return { item: f.item, port: 'out' };
  if (near(tl[0], tl[1] + off)) return { item: f.item, port: 'in' };
  return null;
}

/** Text frame in and out ports; red plus when the story is overset. */
function Ports({ box, s, color, text }: { box: [number, number][]; s: number; color: RGB | null; text: NonNullable<ItemInfo['text']> }) {
  const stroke = css(color);
  const [tl, , br] = box.map(([x, y]) => [x * s, y * s]);
  const inY = tl[1] + Math.min(18, (br[1] - tl[1]) / 3);
  const outY = br[1] - Math.min(18, (br[1] - tl[1]) / 3);
  const port = (x: number, y: number, key: string, mark: React.ReactNode, over = false) => (
    <g key={key}>
      <rect className={over ? 'port over' : 'port'} stroke={over ? '#e0302a' : stroke} x={Math.round(x) - 4.5} y={Math.round(y) - 4.5} width={9} height={9} />
      {mark}
    </g>
  );
  return (
    <g>
      {port(tl[0], inY, 'in', text.previous ? <path d={`M${tl[0] - 2},${inY - 2.5} l3,2.5 l-3,2.5z`} fill={stroke} /> : null)}
      {port(
        br[0],
        outY,
        'out',
        text.overset ? (
          <path d={`M${br[0] - 3},${outY} h6 M${br[0]},${outY - 3} v6`} stroke="#e0302a" strokeWidth={1.4} />
        ) : text.next ? (
          <path d={`M${br[0] - 2},${outY - 2.5} l3,2.5 l-3,2.5z`} fill={stroke} />
        ) : null,
        text.overset,
      )}
    </g>
  );
}

