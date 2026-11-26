// Read-only views of the open document for the editor UI: layers, swatches, masters, styles
// (documentInfo) and each page's items, guides and columns (pageItems). Coordinates are
// points from the page's top-left, y down, like preflight bounds.

export type RGB = [number, number, number];
export type Box = [number, number, number, number];
export type Matrix = [number, number, number, number, number, number];

export type ViewInfo = {
  strokeUnits: string; cursorKeyIncrement: number; leadingKeyIncrement: number; baselineShiftKeyIncrement: number; kerningKeyIncrement: number; snapZone: number;
  rulerGuides: { threshold: number; color: RGB; colorName: string | null };
  pasteboard: {
    horizontal: number; vertical: number; matchPreviewBackground: boolean;
    previewColor: RGB; previewColorName: string | null; bleedColor: RGB; bleedColorName: string | null; slugColor: RGB; slugColorName: string | null;
  };
  guidesLocked: boolean; guidesSnap: boolean; guidesInBack: boolean; columnGuidesLocked: boolean; gridsInBack: boolean;
  baselineGridShown: boolean; documentGridShown: boolean; documentGridSnap: boolean;
  baseline: { start: number; division: number; threshold: number; relativeToMargin: boolean; color: RGB; colorName: string | null };
  grid: { horizontal: number; vertical: number; horizontalSub: number; verticalSub: number; color: RGB; colorName: string | null };
};

export type DocPage = { id: string; index: number; master: string | null; spreadIndex: number; spreadOffset: [number, number]; spreadOrigin?: [number, number]; side?: PageSide };

/** A page's facts by its page index (a master's pages are numbered from 1,000,000, so the index is not the position). */
export const pageOf = (info: { pages: DocPage[] } | null | undefined, index: number): DocPage | undefined => info?.pages.find((p) => p.index === index);

export type DocumentInfo = {
  hyperlinks?: import('./features/hyperlinks/types').HyperlinkData;
  strokeStyles?: import('./features/strokes/types').StrokeStyle[];
  tocStyles?: import('./features/toc/types').TocStyle[];
  tocStories?: Record<string, string>;
  /** Data Merge ▸ Select Data Source: the data source the document remembers (`DataMerge` of its preferences: the project reference `folio:path-ref/<id>` and InDesign's file type), none when it has none. */
  dataMergeSource?: { file: string; fileType: string | null } | null;
  /** Data Merge computed fields of the document (label `folio.datamerge`): `setDataMergeComputedFields` sets the list; the panel reads the source again when it changes. */
  dataMergeComputed?: { name: string; expr: string }[];
  editorial?: import('./features/editorial/types').EditorialStory[];
  footnoteOptions?: Record<string, string>;
  endnoteOptions?: Record<string, string>;
  /** Object ▸ Primary Text Frame: the text frames of the parents. `primary`: those of the threads that are a parent's primary text frame;
   *  `empty`: those whose story has no text (the only ones the command acts on). */
  parentTextFrames?: { primary: string[]; empty: string[] };
  documentPreferences?: Record<string, string | number | boolean>;
  fileInfo?: Record<string, string>;
  colorPreferences?: Record<string, string | number | boolean>;
  /** The fill and stroke the next object drawn gets (`PageItemDefault`); the toolbox chips show them with nothing selected, `setDefaultFillStroke` sets them. */
  defaultFillStroke?: { fill: Paint & { id: string }; stroke: Paint & { id: string } };
  languageQuotes?: { id: string; name: string; singleQuotes: string; doubleQuotes: string }[];
  /** The languages of the Character panel's Language box: the name in the file and the name shown. */
  languageChoices?: { name: string; label: string }[];
  /** Swatches panel colour groups (designmap order; the root group holds the ungrouped swatches); absent when the document cannot be edited. */
  colorGroups?: { id: string; name: string; root: boolean; swatches: string[] }[];
  /** Mixed ink groups: the group's id (`MixedInkGroup/<name>`) to the ids of its swatches. */
  mixedInkGroups?: Record<string, string[]>;
  activeLayer?: string | null;
  units: { horizontal: string; vertical: string };
  rulerOrigin: string;
  /** `Document/@ZeroPoint`: the point of the ruler that reads 0 (ruler origin coordinates). */
  zeroPoint?: [number, number];
  /** View ▸ Grids & Guides: the document's toggles and what its grids look like. */
  view?: ViewInfo;
  /** Top layer first, as the Layers panel lists them. */
  layers: { id: string; name: string; visible: boolean; printable: boolean; locked: boolean; color: RGB; colorName?: string | null; showGuides?: boolean; lockGuides?: boolean; items: number }[];
  swatches: { id: string; name: string; model: string; space: string; values: number[]; tint: number | null; base: string | null; rgb: RGB; stops?: {color: string; location: number; midpoint: number}[] }[];
  masters: { id: string; name: string; prefix: string; baseName: string; pages: number; basedOn?: string | null }[];
  facingPages?: boolean;
  pages: DocPage[];
  /** The pages of the master spreads, numbered from 1,000,000 (the editor opens a master like a document). */
  masterPages?: (DocPage & { name: string; masterSpread: string; size: [number, number] })[];
  paragraphStyles: { id: string; name: string; group: string[]; used?: boolean }[];
  characterStyles: { id: string; name: string; group: string[]; used?: boolean }[];
  /** `used`: applied to an item of a page or of a parent page. */
  objectStyles: { id: string; name: string; group: string[]; used?: boolean }[];
  /** Cell and table styles with the style each is based on and the settings it gives (IDML attributes). */
  cellStyles: TableStyleInfo[];
  tableStyles: TableStyleInfo[];
  /** Every cell style group and table style group (empty ones too): `id` is the group's `Self`, `path` the group names from the top. */
  /** The styles that have a keyboard shortcut; the style that wins a key press comes first. */
  styleShortcuts?: { kind: 'paragraph' | 'character'; id: string; name: string; shortcut: string; label: string }[];
  paragraphStyleGroups?: StyleGroupInfo[];
  characterStyleGroups?: StyleGroupInfo[];
  cellStyleGroups?: StyleGroupInfo[];
  objectStyleGroups?: StyleGroupInfo[];
  tableStyleGroups?: StyleGroupInfo[];
  /** The Ink Manager's inks: process first, then spots; `alias` = the ink it prints on, `inkType` Normal, Transparent, Opaque or OpaqueIgnore. */
  inks: { name: string; process: boolean; convertToProcess: boolean; alias: string | null; inkType: string; print: boolean }[];
  /** The document's text variables (the Insert Variable menu): `id` is the variable's `Self`. */
  textVariables: { id: string; name: string; kind: string; /** The settings as the designmap has them (`TextBefore`, `SearchStrategy`, a custom text's `Contents`). */ settings: Record<string, string> }[];
};

export type TableStyleInfo = { id: string; name: string; group?: string[]; basedOn: string; settings: Record<string, string>; /** applied to a table or a cell (the engine reads it; Select All Unused) */ used?: boolean };
export type StyleGroupInfo = { id: string; path: string[] };

/** `space`, `values` and `color` (its `Self`) are there for an unnamed colour (the Color panel's sliders make these). */
export type Paint = { kind: 'none' | 'swatch' | 'gradient' | 'other'; swatch: string | null; tint: number | null; rgb: RGB | null; space?: string; values?: number[]; color?: string };

/** IDML `LocalDisplaySetting`: Default (use the view's), HighQuality, Typical, Optimized (Fast Display). */
export type LocalDisplay = 'Default' | 'HighQuality' | 'Typical' | 'Optimized';

export type ItemKind = 'textFrame' | 'rectangle' | 'oval' | 'polygon' | 'line' | 'group' | 'other';

export type CornerShape = 'None' | 'RoundedCorner' | 'InverseRoundedCorner' | 'InsetCorner' | 'BevelCorner' | 'FancyCorner';
export type Corner = { shape: CornerShape; radius: number };
export type BlendLevel = 'object' | 'stroke' | 'fill' | 'text' | 'graphic';
/** Text Frame Options (IDML names for the choices); insets top, left, bottom, right. */
export type FrameOptions = {
  gutter: number; fixedWidth: number | null; insets: [number, number, number, number]; vertical: string; ignoreWrap: boolean; firstBaseline: string; minFirstBaseline: number;
  /** Auto-Size: Off or the IDML type; the reference point; the minimums when used; No Line Breaks. */
  autoSize: string; autoSizePoint: string; minHeight: number | null; minWidth: number | null; noLineBreaks: boolean;
};
export type Blend = { opacity: number; mode: string };

export type ItemInfo = {
  anchored?: boolean;
  text_wrap?:{mode:'BoundingBox'|'Contour'|'JumpObject'|'NextColumn';offsets:[number,number,number,number];inverse:boolean;side:string;master_only?:boolean}|null;
  id: string;
  kind: ItemKind;
  name: string | null;
  layer: string | null;
  locked: boolean;
  visible: boolean;
  master: boolean;
  /** A parent item that pages may override (Allow Parent Item Overrides). */
  allow_overrides: boolean;
  /** Object ▸ Display Performance: `LocalDisplaySetting` (`Default` follows the view). */
  local_display?: LocalDisplay;
  fitting?: import('./features/objopts/types').FrameFitting;
  /** Attributes panel: Nonprinting, Overprint Fill / Stroke / Gap. */
  print?: { nonprinting: boolean; overprint_fill: boolean; overprint_stroke: boolean; overprint_gap: boolean };
  /** Draw order in the spread, 0 = bottom. */
  z: number;
  /** Inner → page coordinates. */
  transform: Matrix;
  /** Geometric bounds in inner coordinates. */
  inner: Box;
  /** Axis-aligned geometric bounds in page coordinates. */
  bounds: Box;
  outline: [number, number][][] | null;
  /** The path points in the item's own space (`[anchor x, y, left x, y, right x, y]`), for shapes with a modest number of them. */
  paths?: { open: boolean; points: [number, number, number, number, number, number][] }[] | null;
  fill: Paint;
  fill_gradient?: {start: {x: number; y: number}; length: number; angle: number} | null;
  stroke_gradient?: {start: {x: number; y: number}; length: number; angle: number} | null;
  /** cap, join, type, start and end as IDML names them; type is the stroke style's `Self`. */
  stroke: Paint & {
    weight: number; alignment: string; cap: string; join: string; miterLimit: number; type: string;
    start: string; end: string; startScale: number; endScale: number; gap: Paint;
  };
  opacity: number;
  /** Corner options: top left, top right, bottom left, bottom right; shape as IDML names it. */
  corners?: Corner[];
  /** Opacity (percent) and blend mode (IDML name) of each Effects panel level. */
  blending?: Record<BlendLevel, Blend>;
  /** Group transparency (Effects panel): Isolate Blending, Knockout Group. */
  isolate?: boolean;
  knockout?: boolean;
  /** The applied object style (`Self`, e.g. "ObjectStyle/Oracle Box"). */
  style?: string | null;
  content: { image_space?: string | null; image_color?: import('./features/objopts/types').ImageColor | null; kind: string; clipping?: boolean; layer_options?: import('./features/objopts/types').GraphicLayers | null; clipping_options?: import('./features/objopts/types').ClippingOptions | null; transform: Matrix; inner: Box; bounds: Box; link: string | null; ppi: [number, number] | null; local_display?: LocalDisplay } | null;
  text: { story: string; previous: string | null; next: string | null; columns: number; overset: boolean; empty?: boolean; /** Type ▸ Type Orientation of the story */ orientation?: 'Horizontal' | 'Vertical'; options?: FrameOptions } | null;
  children: ItemInfo[];
};

export type Guide = { id: string; spread: boolean; master?: boolean; orientation: 'horizontal' | 'vertical'; location: number; fitToPage: boolean; color: RGB; layer: string | null; locked: boolean; ownLocked: boolean; viewThreshold: number; colorName: string | null; guideType: string };

export type PageSide = 'left' | 'right' | 'single';

export type PageItems = {
  page: number;
  side?: PageSide;
  size: [number, number];
  margins: [number, number, number, number];
  columns: { count: number; gutter: number; positions: number[] | null; direction: string };
  guides: Guide[];
  items: ItemInfo[];
  masterItems: ItemInfo[];
};

const unknownCmd = (e: unknown) => String(e ?? '').startsWith('unknown cmd');

/** null when the engine predates these commands. */
export async function getDocumentInfo(): Promise<DocumentInfo | null> {
  const { header } = await window.folio.request({ cmd: 'documentInfo' });
  if (!header.ok) {
    if (unknownCmd(header.error)) return null;
    throw new Error(header.error ?? 'documentInfo failed');
  }
  return header.info as DocumentInfo;
}

async function fetchPageItems(page: number): Promise<PageItems | null> {
  const { header } = await window.folio.request({ cmd: 'pageItems', page });
  if (!header.ok) {
    if (unknownCmd(header.error)) return null;
    throw new Error(header.error ?? 'pageItems failed');
  }
  return header.items as PageItems;
}

/** Page items of the open document, fetched once per page; listeners hear when a page arrives. */
export class ItemStore {
  private pages = new Map<number, PageItems | null>();
  private stale = new Set<number>();
  private pending = new Map<number, Promise<PageItems | null>>();
  private listeners = new Set<() => void>();
  /** Per page: bumped by `invalidate`, so a reply from before an edit is not kept. */
  private versions = new Map<number, number>();
  /** false once the engine said it has no pageItems. */
  supported = true;
  generation = 0;

  reset() {
    this.pages.clear();
    this.stale.clear();
    this.pending.clear();
    this.versions.clear();
    this.supported = true;
    this.generation++;
    this.emit();
  }

  /** Pages an edit changed: read again (those already read), older answers dropped. */
  invalidate(pages: number[]) {
    for (const page of pages) {
      const had = this.pages.has(page) || this.pending.has(page);
      // Keep the displayed snapshot until its replacement arrives. Controls and
      // hit testing still need the page while the engine is answering.
      this.stale.add(page);
      this.pending.delete(page);
      this.versions.set(page, (this.versions.get(page) ?? 0) + 1);
      if (had) void this.load(page);
    }
    this.emit();
  }

  peek(page: number): PageItems | null | undefined {
    return this.pages.get(page);
  }

  load(page: number): Promise<PageItems | null> {
    if (this.pages.has(page) && !this.stale.has(page)) return Promise.resolve(this.pages.get(page) ?? null);
    const running = this.pending.get(page);
    if (running) return running;
    const gen = this.generation;
    const version = this.versions.get(page) ?? 0;
    const p = fetchPageItems(page)
      .then((r) => {
        if (gen !== this.generation || version !== (this.versions.get(page) ?? 0)) return null;
        if (r === null) this.supported = false;
        this.pages.set(page, r);
        this.stale.delete(page);
        this.pending.delete(page);
        this.emit();
        return r;
      })
      .catch((e) => {
        if (gen !== this.generation || version !== (this.versions.get(page) ?? 0)) return null;
        this.pending.delete(page);
        console.error('pageItems failed', page, String(e));
        return null;
      });
    this.pending.set(page, p);
    return p;
  }

  subscribe(fn: () => void) {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  }

  private emit() {
    for (const fn of this.listeners) fn();
  }
}

// ---------------------------------------------------------------------------------------
// geometry

export const apply = (m: Matrix, x: number, y: number): [number, number] => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];

export function invert(m: Matrix): Matrix | null {
  const [a, b, c, d, e, f] = m;
  const det = a * d - b * c;
  if (Math.abs(det) < 1e-12) return null;
  const ia = d / det, ib = -b / det, ic = -c / det, id = a / det;
  return [ia, ib, ic, id, -(ia * e + ic * f), -(ib * e + id * f)];
}

/** An item's own transform, relative to its group: `m` (inner → page) taken through the inverse of the group's page matrix. This is what the
 *  Transform panel shows for an item inside a group while "Transformations are Totals" is off (rules/transform-totals.ko.md). */
export function relativeTransform(parent: Matrix, m: Matrix): Matrix | null {
  const i = invert(parent);
  if (!i) return null;
  return [
    i[0] * m[0] + i[2] * m[1], i[1] * m[0] + i[3] * m[1],
    i[0] * m[2] + i[2] * m[3], i[1] * m[2] + i[3] * m[3],
    i[0] * m[4] + i[2] * m[5] + i[4], i[1] * m[4] + i[3] * m[5] + i[5],
  ];
}

/** How far an item's stroke reaches outside its path (InDesign's W/H and X/Y include it:
 *  "Dimensions Include Stroke Weight", on by default). */
export function strokeOutset(it: ItemInfo): number {
  const st = it.stroke;
  if (st.kind === 'none' || !(st.weight > 0)) return 0;
  if (st.alignment === 'InsideAlignment') return 0;
  return st.alignment === 'OutsideAlignment' ? st.weight : st.weight / 2;
}

/** The inner box's corners in page coordinates: top-left, top-right, bottom-right, bottom-left. */
export function corners(m: Matrix, b: Box): [number, number][] {
  return [apply(m, b[0], b[1]), apply(m, b[2], b[1]), apply(m, b[2], b[3]), apply(m, b[0], b[3])];
}

/** InDesign's transform readout: scale (1 = 100 %), rotation and shear in degrees (counter-clockwise positive). */
export function decompose(m: Matrix) {
  const [a, b, c, d] = m;
  const sx = Math.hypot(a, b);
  const det = a * d - b * c;
  const sy = sx ? det / sx : 0;
  const rotation = -(Math.atan2(b, a) * 180) / Math.PI;
  const shear = sx && det ? -(Math.atan2(a * c + b * d, det) * 180) / Math.PI : 0;
  return { sx, sy: Math.abs(sy), flipped: det < 0, rotation: clean(rotation), shear: clean(shear) };
}

const clean = (deg: number) => {
  const r = Math.round(deg * 1000) / 1000;
  return Object.is(r, -0) || Math.abs(r) < 1e-9 ? 0 : r;
};

/** Whether a point is inside a polygon (even-odd). */
export function inside(pt: [number, number], poly: [number, number][]) {
  let hit = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if (yi > pt[1] !== yj > pt[1] && pt[0] < ((xj - xi) * (pt[1] - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/** Distance from a point to a polyline (closed when `closed`). */
export function distance(pt: [number, number], poly: [number, number][], closed: boolean) {
  let best = Infinity;
  const n = poly.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const a = poly[i], b = poly[(i + 1) % n];
    const dx = b[0] - a[0], dy = b[1] - a[1];
    const len = dx * dx + dy * dy;
    const t = len ? Math.max(0, Math.min(1, ((pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / len)) : 0;
    best = Math.min(best, Math.hypot(pt[0] - (a[0] + t * dx), pt[1] - (a[1] + t * dy)));
  }
  return best;
}

/** Margins as drawn: [top, left, bottom, right] on the page; IDML's Left/Right are inside/outside on a left-hand page. */
export function physicalMargins(m: [number, number, number, number], side: PageSide | undefined): [number, number, number, number] {
  return side === 'left' ? [m[0], m[3], m[2], m[1]] : m;
}

export const unionBox = (boxes: Box[]): Box | null =>
  boxes.length
    ? [Math.min(...boxes.map((b) => b[0])), Math.min(...boxes.map((b) => b[1])), Math.max(...boxes.map((b) => b[2])), Math.max(...boxes.map((b) => b[3]))]
    : null;

// ---------------------------------------------------------------------------------------
// measurement units (IDML ViewPreference names)

type Unit = { perPoint: number; suffix: string; decimals: number; kind?: 'picas' | 'ciceros' };

const MM = 72 / 25.4;
const UNITS: Record<string, Unit> = {
  Points: { perPoint: 1, suffix: 'pt', decimals: 3 },
  Millimeters: { perPoint: 1 / MM, suffix: 'mm', decimals: 3 },
  Centimeters: { perPoint: 1 / (MM * 10), suffix: 'cm', decimals: 4 },
  Inches: { perPoint: 1 / 72, suffix: 'in', decimals: 4 },
  InchesDecimal: { perPoint: 1 / 72, suffix: 'in', decimals: 4 },
  Picas: { perPoint: 1 / 12, suffix: 'p', decimals: 3, kind: 'picas' },
  Ciceros: { perPoint: 1 / 12.7878, suffix: 'c', decimals: 3, kind: 'ciceros' },
  Pixels: { perPoint: 1, suffix: 'px', decimals: 0 },
  Q: { perPoint: 1 / (MM * 0.25), suffix: 'Q', decimals: 2 },
  Ha: { perPoint: 1 / (MM * 0.25), suffix: 'H', decimals: 2 },
  Agates: { perPoint: 14 / 72, suffix: 'ag', decimals: 3 },
  AmericanPoints: { perPoint: 72.27 / 72, suffix: 'pt', decimals: 3 },
};

export function unitOf(name: string | undefined): Unit {
  const key = (name ?? 'Points').replace(/\s+/g, '');
  return UNITS[key] ?? UNITS.Points;
}

/** A length in points as InDesign shows it in fields: "210 mm", "12.7 mm", "51p0". */
export function formatLength(pt: number, unitName: string | undefined): string {
  const u = unitOf(unitName);
  if (u.kind) {
    const per = u.kind === 'picas' ? 12 : 12.7878;
    const sign = pt < 0 ? '-' : '';
    const abs = Math.abs(pt);
    let whole = Math.floor(abs / per + 1e-9);
    let rest = Math.round(((abs - whole * per) * 1000)) / 1000;
    if (rest >= per - 1e-6) { whole += 1; rest = 0; }
    return `${sign}${whole}${u.suffix}${trim(rest, 3)}`;
  }
  return `${trim(pt * u.perPoint, u.decimals)} ${u.suffix}`;
}

/** A length typed in a field, in points: a number in the field's unit, or with its own
 *  ("12 mm", "1in", "3p6", "6pt", "2c3"); simple sums ("10mm+2") as InDesign takes them. */
export function parseLength(text: string, unitName: string | undefined): number | null {
  const terms = text.trim().replace(/,/g, '.').match(/[+-]?[^+-]+/g);
  if (!terms) return null;
  let total = 0;
  for (const raw of terms) {
    const t = raw.trim();
    const pica = t.match(/^([+-]?)(\d*\.?\d*)([pc])(\d*\.?\d*)$/i);
    if (pica && (pica[2] || pica[4])) {
      const per = pica[3].toLowerCase() === 'p' ? 12 : 12.7878;
      const v = (parseFloat(pica[2] || '0') * per + parseFloat(pica[4] || '0') * (pica[3].toLowerCase() === 'p' ? 1 : 12.7878 / 12)) * (pica[1] === '-' ? -1 : 1);
      total += v;
      continue;
    }
    const m = t.match(/^([+-]?\d*\.?\d+)\s*(mm|cm|pt|in|px|q|h|ag|"|)$/i);
    if (!m) return null;
    const v = parseFloat(m[1]);
    const suffix = m[2].toLowerCase();
    const per: Record<string, number> = { mm: MM, cm: MM * 10, pt: 1, in: 72, '"': 72, px: 1, q: MM / 4, h: MM / 4, ag: 72 / 14 };
    total += suffix ? v * per[suffix] : v / unitOf(unitName).perPoint;
  }
  return Number.isFinite(total) ? total : null;
}

export const trim = (v: number, decimals: number) => {
  const f = 10 ** decimals;
  const r = Math.round(v * f) / f;
  return String(Object.is(r, -0) ? 0 : r);
};

/** Ruler ticks: label step and subdivisions, in the unit, for a scale in screen px per point. */
export function rulerSteps(unitName: string | undefined, pxPerPoint: number) {
  const u = unitOf(unitName);
  const pxPerUnit = pxPerPoint / u.perPoint;
  const bases = u.kind ? [1, 2, 3, 6, 12, 24, 48, 96, 192] : [1, 2, 5];
  let major = 1;
  if (u.kind) {
    // picas: a pica is 12 points; steps in picas
    major = bases.find((b) => b * pxPerUnit >= 48) ?? 384;
  } else {
    let p = 1e-3;
    outer: for (; p < 1e6; p *= 10) {
      for (const b of bases) {
        if (b * p * pxPerUnit >= 52) { major = b * p; break outer; }
      }
    }
  }
  const majorPx = major * pxPerUnit;
  const minor = majorPx >= 80 ? 10 : majorPx >= 40 ? 5 : 2;
  return { major, minor, pxPerUnit, unit: u };
}
