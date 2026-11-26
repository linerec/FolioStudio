// Engine client: requests go through the preload bridge to the Rust engine process.
import type { RegisteredEditCommand } from './generated/edit-commands';
import { engineMessage, type EngineMessage } from './i18n/engine-message';

export type PageInfo = { id?: string; index: number; name: string; width: number; height: number; spread: string };
export type PageSection = { page: string; start: number; continue: boolean; style: string; prefix: string; includePrefix: boolean; marker: string };
/** Pages panel ▸ Page Attributes ▸ Spread Flattening of a spread: the level and the custom values (kept at every level). */
export type SpreadFlattener = { level: 'Default' | 'None' | 'Custom'; balance: number; lineArt: number; gradient: number; clip: boolean; strokes: boolean; text: boolean };
export type PageSettings = {
  width: number; height: number; allowShuffle: boolean; spreads: Record<string, boolean>;
  /** Spreads hidden by Layout ▸ Pages ▸ Hide Spread. */
  hidden?: Record<string, boolean>;
  /** Spreads turned by View ▸ Rotate Spread: the quarter turn clockwise (90, 180 or 270); an upright spread is left out. */
  rotation?: Record<string, number>;
  /** Spreads whose pages do not show their parent's items (Show Parent Items off). */
  hideParentItems?: Record<string, boolean>;
  flattener?: Record<string, SpreadFlattener>;
  /** Color Label of a page when it is not "use the parent's" (IDML name, or Nothing); of a parent likewise. */
  pageColors?: Record<string, string>;
  parentColors?: Record<string, string>;
  /** Screen colour of each label colour in use. */
  labelColors?: Record<string, [number, number, number]>;
  sections: PageSection[];
};

export type DocumentSummary = {
  path: string;
  bytes: number;
  pages: PageInfo[];
  pageSettings?: PageSettings | null;
  stories: number;
  warnings: string[];
  missingFonts: [string, string][];
  overset: number;
  layout: { lines: number; glyphs: number };
  timings: Record<string, number>;
  color: { cmyk: ProfileUse; rgb: ProfileUse } | null;
  /** Document bleed in points. */
  bleed?: { top: number; bottom: number; inside: number; outside: number };
  slug?: { top: number; bottom: number; inside: number; outside: number };
  /** A .folio document: its project, references and lock (project.tsx); null for INDD/IDML. */
  folio?: import('./project').FolioSummary | null;
  /** The engine can edit it (a .folio whose lock this window holds). */
  editable?: boolean;
  /** A master spread opened like a document (Pages panel, double click): its name; `pages` are then its pages. */
  masterName?: string;
};

// ---------------------------------------------------------------------------------------
// editing (folio-edit; each command changes what InDesign changes for the same action)

export type Anchor = 'topLeft' | 'top' | 'topRight' | 'left' | 'center' | 'right' | 'bottomLeft' | 'bottom' | 'bottomRight';
export type PdfOptions = { pageNumber: number; crop: 'CropContentVisibleLayers' | 'CropContentAllLayers' | 'CropArt' | 'CropPDF' | 'CropTrim' | 'CropBleed' | 'CropMedia'; transparentBackground: boolean };
export type PlaceInfo = { format: string | null; unsupported?: string | null; pdf: boolean; pages: number | null; width: number; height: number; pageBoxes: { media: number[]; crop: number[] | null; art: number[] | null; trim: number[] | null; bleed: number[] | null; rotate: number }[]; fonts?: { name: string; kind: string; embedded: boolean }[] };
export const getPlaceInfo = (file: string) => call<PlaceInfo>({ cmd: 'placeInfo', file }, 'info');

/** What placing a text file (.txt, .docx, .rtf) puts into a story, without placing it: the styles and swatches a Word or RTF file brings, and what was not carried over. */
export type TextImportInfo = { format: string; paragraphs: number; characters: number; endsWithReturn: boolean; warnings: string[]; warningMessages?: import('./i18n/engine-message').EngineMessage[]; preview: string; paragraphStyles: string[]; characterStyles: string[]; colors: string[];
  /** A plain text file: the character set it is read with (`AUTO`: the one found) and the one its bytes look like. */
  characterSet?: string | null; detectedCharacterSet?: string | null };
/** The XMP properties of a file (`fileMetadata`); an empty string or list: the file does not say it. */
export type XmpFieldsInfo = { title: string; description: string; creator: string[]; rights: string; keywords: string[]; headline: string; credit: string;
  source: string; city: string; state: string; country: string; authorTitle: string; captionWriter: string; instructions: string; dateCreated: string;
  creatorTool: string; rightsUrl: string; location: string; rating: string; captureDate: string; camera: string; lens: string };
export type FileMetadata = { size: number; modified: number | null; packet: string | null; fields: XmpFieldsInfo | null };
export const getFileMetadata = (file: string) => call<FileMetadata>({ cmd: 'fileMetadata', file }, 'info');
export const getTextImportInfo = (file: string, options?: Record<string, unknown>) => call<TextImportInfo>({ cmd: 'textImportInfo', file, ...(options ? { options } : {}) }, 'info');

export type AlignMode = 'left' | 'hCenter' | 'right' | 'top' | 'vCenter' | 'bottom' | 'hSpace' | 'vSpace';
export type AlignBounds = 'selection' | 'margin' | 'page' | 'spread' | 'key' | 'bleed';
export type DrawKind = 'rectangle' | 'ellipse' | 'polygon' | 'line' | 'rectangleFrame' | 'ellipseFrame' | 'polygonFrame' | 'textFrame';

export type EditCommand =
  | import('./objectstyles.generated').ObjectStyleCommand
  | RegisteredEditCommand
  | import('./features/color/commands.generated').ColorCommand
  | { op: 'threadText'; id: string; next: string | null }
  | { op: 'createThreadFrame'; id: string; page: string; rect: [number, number, number, number]; layer?: string }
  | { op: 'placeText'; file: string; options?: { characterSet?: string; removeFormatting?: boolean; convertPageBreaks?: 'PAGE_BREAK' | 'COLUMN_BREAK' | 'NONE'; useTypographersQuotes?: boolean; stripReturnsBetweenLines?: boolean; stripReturnsBetweenParagraphs?: boolean; convertSpacesIntoTabs?: boolean; spacesIntoTabsCount?: number; preserveGraphics?: boolean; fillMissingGlyphs?: boolean }; id?: string; page?: string; point?: [number, number]; rect?: [number, number, number, number]; layer?: string }
  | { op: 'placeGraphic'; file: string; pdf?: PdfOptions; options?: { renderingIntent?: string }; id?: string; page?: string; point?: [number, number]; rect?: [number, number, number, number]; layer?: string }
  | { op: 'insertPages'; count: number; before?: string; master?: string | null }
  | { op: 'overrideParentItems'; page: string; ids?: string[] }
  | { op: 'rotateAbout'; ids: string[]; by: number; point: [number, number] }
  | { op: 'makeCompoundPath'; ids: string[] }
  | { op: 'releaseCompoundPath'; id: string }
  | { op: 'relinkGraphic'; id: string; file: string }
  | { op: 'embedLink'; id: string; file: string }
  | { op: 'unembedLink'; id: string; uri?: string }
  | { op: 'setPageColor'; pages?: string[]; masters?: string[]; color: string }
  | { op: 'setSpreadShowMaster'; spreads: string[]; show: boolean }
  | { op: 'setSpreadFlattener'; spreads: string[]; level?: 'Default' | 'None' | 'Custom'; balance?: number; lineArt?: number; gradient?: number; clip?: boolean; strokes?: boolean; text?: boolean }
  | { op: 'removeLocalOverrides'; page: string; ids?: string[] }
  | { op: 'detachFromParent'; page: string; ids?: string[] }
  | { op: 'allowParentOverrides'; ids: string[]; allow: boolean }
  | { op: 'deletePages'; pages: string[] }
  | { op: 'movePages'; pages: string[]; before?: string }
  | { op: 'setSpreadHidden'; spreads: string[]; hidden: boolean }
  | { op: 'setPageShuffle'; allowed: boolean; spread?: string }
  | { op: 'duplicateSpread'; spread: string }
  | { op: 'applyMaster'; pages: string[]; master?: string | null }
  | { op: 'setSection'; page: string; start?: number; style: string; prefix: string; includePrefix: boolean; marker: string }
  | { op: 'documentSetup'; width: number; height: number }
  | {op:'findReplace';query:FindQuery;text?:string;props?:TextProperties}
  | {op:'textWrap';id:string;mode?:string;offsets?:[number,number,number,number];inverse?:boolean;masterOnly?:boolean}
  | {op:'styleCreate';kind:'paragraph'|'character';name:string;story?:string;start?:number;end?:number;props?:Record<string,string|number|boolean>;apply?:boolean;group?:string}
  | {op:'styleEdit';kind:'paragraph'|'character';style:string;props:Record<string,string|number|boolean>}
  | {op:'styleRedefine';kind:'paragraph'|'character';style:string;story:string;start:number;end:number}
  | {op:'styleDelete';kind:'paragraph'|'character';style:string;replacement:string}
  | { op: 'paragraphStyle' | 'characterStyle'; story: string; start: number; end: number; style: string; clear?: boolean }
  | { op: 'textFormat'; story: string; start: number; end: number; props: TextProperties }
  | { op: 'pasteText'; story: string; at: number; fragment: TextFragment }
  | { op: 'createItem'; page: string; kind: DrawKind; rect: [number, number, number, number]; layer?: string; sides?: number }
  | { op: 'insertText'; story: string; at: number; text: string }
  /** Layers panel ▸ Duplicate Layer / Merge Layers / Delete Unused Layers. */
  | { op: 'duplicateLayer'; id: string }
  | { op: 'mergeLayers'; into: string; ids: string[] }
  | { op: 'deleteUnusedLayers' }
  /** Layers panel ▸ Unlock All: every object lock in the document (`page`: only that page's spread). */
  | { op: 'unlockAll'; page?: string }
  /** Layout ▸ Margins and Columns (IDML Left/Right: inside/outside on facing pages). */
  | { op: 'setMargins'; pages: string[]; top?: number; bottom?: number; left?: number; right?: number; columns?: number; gutter?: number }
  /** Layout ▸ Create Guides on every page of the page's spread. */
  | { op: 'createGuides'; page: string; rows: number; columns: number; rowGutter: number; columnGutter: number; fitMargins: boolean; removeExisting: boolean }
  /** Type ▸ Insert Special Character / White Space / Break Character (InDesign's scripting name). */
  | { op: 'insertSpecial'; story: string; at: number; special: string }
  /** Type ▸ Change Case of [start, end). */
  | { op: 'changeCase'; story: string; start: number; end: number; mode: 'UPPERCASE' | 'LOWERCASE' | 'TITLECASE' | 'SENTENCECASE' }
  | { op: 'deleteText'; story: string; start: number; end: number }
  | { op: 'move'; ids: string[]; by: [number, number] }
  | { op: 'moveToSpread'; ids: string[]; spread: string; by: [number, number] }
  | { op: 'align'; ids: string[]; how: AlignMode; bounds: AlignBounds; key?: string }
  | { op: 'distribute'; ids: string[]; how: AlignMode; bounds: AlignBounds; key?: string; gap?: number }
  /** Control panel W/H (stroke included) about the reference point. */
  | { op: 'resize'; id: string; size: [number, number]; anchor: Anchor; includeStroke?: boolean }
  /** A dragged handle: the path's new bounds in the item's own space. */
  | { op: 'resizeBox'; id: string; rect: [number, number, number, number] }
  | { op: 'resizeMany'; ids: string[]; size: [number, number]; anchor: Anchor; includeStroke?: boolean }
  | { op: 'delete'; ids: string[] }
  | { op: 'setItem'; id: string; locked?: boolean; visible?: boolean }
  | { op: 'setLayer'; id: string; visible?: boolean; locked?: boolean; printable?: boolean; name?: string; color?: string; showGuides?: boolean; lockGuides?: boolean }
  | { op: 'newLayer'; name?: string; color?: string; above?: string }
  | { op: 'newSwatch'; name: string; spot: boolean; space: string; value: number[] }
  | { op: 'setSwatch'; id: string; value?: number[]; space?: string; spot?: boolean; rename?: string }
  | { op: 'deleteSwatch'; id: string; replace: string }
  | { op: 'newTint'; base: string; tint: number }
  | { op: 'setInk'; name: string; inkType?: string; convertToProcess?: boolean; /** the ink to print on; '' for none */ alias?: string }
  | { op: 'allSpotsToProcess'; on: boolean }
  | { op: 'duplicateSwatch'; id: string }
  /** A ruler guide `at` points from the top (horizontal) or left (vertical) edge of `page`. */
  | { op: 'newGuide'; page: string; orientation: 'horizontal' | 'vertical'; at: number; spread?: boolean; layer?: string }
  | { op: 'moveGuide'; id: string; page: string; at: number }
  | { op: 'deleteGuide'; id: string }
  /** The Pen tool: a path through `points` (page coordinates; handles optional): a closed one is a Polygon, an open one a GraphicLine. */
  | { op: 'createPath'; page: string; points: { anchor: [number, number]; left?: [number, number]; right?: [number, number] }[]; closed: boolean; layer?: string }
  /** Direct Selection: the anchors `indices` of the item's `path` move with their handles by `by` (the item's own space). */
  | { op: 'movePathPoints'; id: string; path?: number; indices: number[]; by: [number, number] }
  /** Add Anchor Point: a point `t` (0 to 1) of the way along the segment that starts at point `segment`; the curve keeps its shape. */
  | { op: 'addPathPoint'; id: string; path?: number; segment: number; t: number }
  /** Delete Anchor Point: the point goes and its neighbours keep their handles. */
  | { op: 'deletePathPoint'; id: string; path?: number; index: number }
  /** Convert Direction Point: a click makes the point a plain corner (handles into the anchor). */
  | { op: 'plainPathPoint'; id: string; path?: number; index: number }
  /** Convert Direction Point dragged: both handles of the point to these places (the item's own space). */
  | { op: 'setPathHandles'; id: string; path?: number; index: number; left: [number, number]; right: [number, number] }
  /** Direct Selection: a handle goes to `to` (the item's own space); a smooth point turns its other handle unless `independent`. */
  | { op: 'movePathHandle'; id: string; path?: number; index: number; side: 'left' | 'right'; to: [number, number]; independent?: boolean }
  /** View ▸ Grids & Guides: a document setting (guidesLocked, guidesSnap, guidesInBack, columnGuidesLocked, baselineGridShown, documentGridShown, documentGridSnap, gridsInBack). */
  | { op: 'setViewPref'; key: string; value: boolean | number | string | number[] }
  /** All four corners: top left, top right, bottom left, bottom right. */
  | { op: 'setCorners'; ids: string[]; corners: { shape: string; radius: number }[] }
  /** Opacity (percent) and/or blend mode (IDML name) of one Effects panel level. */
  | { op: 'textFrameOptions'; id: string; options: { columns?: number; gutter?: number; fixed?: boolean; columnWidth?: number; insets?: [number, number, number, number]; vertical?: string; ignoreWrap?: boolean; firstBaseline?: string; minFirstBaseline?: number } }
  | { op: 'newParent'; pages?: number; prefix?: string; name?: string; basedOn?: string }
  | { op: 'duplicateParent'; id: string }
  | { op: 'deleteParent'; id: string }
  /** basedOn: a parent's Self, or "n" for [None]. */
  | { op: 'parentOptions'; id: string; prefix?: string; name?: string; basedOn?: string }
  | { op: 'applyObjectStyle'; ids: string[]; style: string; clear?: boolean }
  | { op: 'clearObjectStyleOverrides'; ids: string[] }
  | { op: 'scale'; id: string; sx: number; sy: number; anchor: Anchor; adjust?: { stroke: boolean; effects: boolean } }
  | { op: 'redefineScaling'; ids: string[] }
  | { op: 'setAngles'; id: string; rotation?: number; shear?: number; anchor: Anchor; content?: boolean; totals?: boolean }
  | { op: 'scaleMany'; ids: string[]; sx: number; sy: number; anchor: Anchor; adjust?: { stroke: boolean; effects: boolean } }
  | { op: 'setStroke'; ids: string[]; change: import('./ui/stroke').StrokeChange }
  | { op: 'setBlending'; ids: string[]; level: 'object' | 'stroke' | 'fill' | 'text' | 'graphic'; opacity?: number; mode?: string; isolate?: boolean; knockout?: boolean }
  | { op: 'setGlobalLight'; angle?: number; altitude?: number }
  | { op: 'setEffect'; ids: string[]; level: import('./ui/effectsdialog').FxLevel; effect: import('./ui/effectsdialog').EffectName; on?: boolean; values: Record<string, number | string | boolean | number[]> }
  | { op: 'moveLayer'; id: string; above?: string; below?: string }
  | { op: 'deleteLayer'; id: string }
  | { op: 'activeLayer'; id: string }
  | { op: 'toLayer'; ids: string[]; layer: string }
  | { op: 'arrange'; id: string; how: 'front' | 'forward' | 'backward' | 'back' }
  // Table menu (cells are [column, row], as InDesign names them)
  | { op: 'insertTable'; story: string; at?: number; rows: number; columns: number; header?: number; footer?: number }
  | { op: 'convertToTable'; story: string; start: number; end?: number }
  | { op: 'insertRows' | 'insertColumns'; table: string; at: number; count: number; before: boolean }
  /** Table ▸ Paste Before / Paste After with the rows (columns) `from`..`to` of `source` (this table without one) copied. */
  | { op: 'setPrimaryTextFrame'; id: string; primary: boolean }
  | { op: 'pasteRows' | 'pasteColumns'; table: string; source?: string; from: number; to: number; at: number; before: boolean }
  | { op: 'deleteRows' | 'deleteColumns'; table: string; at: number; count: number }
  | { op: 'mergeCells'; table: string; from: [number, number]; to: [number, number] }
  | { op: 'unmergeCell'; table: string; cell: [number, number] }
  | { op: 'splitCell'; table: string; cell: [number, number]; vertical: boolean }
  | { op: 'setRows'; table: string; rows: number[]; height?: number; autoGrow?: boolean }
  | { op: 'setColumns'; table: string; columns: number[]; width: number }
  | { op: 'setCells'; table: string; cells: [number, number][]; change: CellChange }
  | { op: 'setCellType'; table: string; cells: [number, number][]; graphic: boolean }
  | { op: 'setHeaderRows'; table: string; header: number }
  /** Table Options ▸ Row Strokes / Column Strokes / Fills: the changed pattern attributes (IDML names and text). */
  | { op: 'setTableOptions'; table: string; options: Record<string, string> }
  /** Cell Styles / Table Styles panels (cells are [column, row]; style ids are `CellStyle/Name`, `CellStyle/$ID/[None]`, `TableStyle/Name`, `TableStyle/$ID/[No table style]`). */
  | { op: 'applyCellStyle'; table: string; cells: [number, number][]; style: string }
  | { op: 'clearCellOverrides'; table: string; cells: [number, number][] }
  | { op: 'applyTableStyle'; table: string; style: string }
  | { op: 'clearTableOverrides'; table: string }
  | { op: 'createTableStyle'; kind: 'cell' | 'table'; name: string; props: Record<string, string>; group?: string }
  | { op: 'editTableStyle'; kind: 'cell' | 'table'; style: string; rename?: string; props: Record<string, string> }
  | { op: 'deleteTableStyle'; kind: 'cell' | 'table'; style: string; replacement: string }
  | { op: 'duplicateTableStyle'; kind: 'cell' | 'table'; style: string; name?: string }
  | { op: 'createTableStyleGroup'; kind: 'cell' | 'table'; name: string; parent?: string }
  | { op: 'renameTableStyleGroup'; kind: 'cell' | 'table'; group: string; name: string }
  | { op: 'deleteTableStyleGroup'; kind: 'cell' | 'table'; group: string }
  | { op: 'moveTableStyle'; kind: 'cell' | 'table'; style: string; group?: string }
  /** Type ▸ Text Variables ▸ Insert Variable: an instance of the variable (its `Self`) at `at` of the story. */
  | { op: 'insertVariable'; story: string; at: number; variable: string }
  /** Type ▸ Text Variables ▸ Define ▸ New / Edit: the variable's kind (`VariableType`) and settings (IDML names), a rename. */
  | { op: 'defineTextVariable'; name: string; kind: string; options: Record<string, string> }
  /** Convert Variable to Text: the instances in [start, end) of the story become the text they show. */
  | { op: 'convertVariableToText'; story: string; start: number; end: number }
  /** Define ▸ Delete: the variable goes, its instances become the text they show. */
  | { op: 'deleteTextVariable'; id: string }
  | { op: 'editTextVariable'; id: string; rename?: string; options: Record<string, string> }
  | { op: 'setFooterRows'; table: string; footer: number }
  | { op: 'convertRows'; table: string; at: number; count: number; to: 'header' | 'body' | 'footer' }
  | { op: 'convertTableToText'; table: string }
  | { op: 'convertShape'; ids: string[]; to: string }
  | { op: 'reversePath'; ids: string[] }
  /** Edit ▸ Paste (`at`: the objects' middle in `page`'s coordinates), Paste in Place, Paste Into. */
  /** `rememberLayers`: Paste Remembers Layers, each object on the layer it was copied from (else the active layer). */
  | { op: 'paste'; page?: string; at?: [number, number]; layer?: string; into?: string; rememberLayers?: boolean }
  | { op: 'distributeRows' | 'distributeColumns'; table: string; first: number; last: number }
  | { op: 'deleteTable'; table: string }
  /** Degrees, counter-clockwise, about the reference point of the objects together. */
  | { op: 'rotate'; ids: string[]; by: number; anchor: Anchor }
  | { op: 'flip'; ids: string[]; axis: 'horizontal' | 'vertical' | 'both'; anchor: Anchor }
  /** Type ▸ Bulleted & Numbered Lists ▸ Apply/Remove Bullets (`numbers` false) / Numbers on the paragraphs of the range. */
  | { op: 'toggleList'; story: string; start: number; end: number; numbers: boolean }
  /** Restart/Continue Numbering of the numbered paragraph the selection is in. */
  | { op: 'toggleNumberingRestart'; story: string; start: number; end: number }
  /** Degrees, positive leans right, along the horizontal axis, about the reference point of the objects together. */
  | { op: 'shear'; ids: string[]; by: number; anchor: Anchor; adjust?: { stroke: boolean; effects: boolean } }
  /** Object ▸ Transform Again (and the sequence / individually variants): repeats the editor's last move, rotation, scale or flip. */
  | { op: 'transformAgain'; ids: string[]; sequence?: boolean; individually?: boolean }
  | { op: 'group'; ids: string[] }
  /** `rememberLayers` (default true): members go back to the layers they had when grouped. */
  | { op: 'ungroup'; id: string; rememberLayers?: boolean }
  | { op: 'duplicate'; ids: string[]; by: [number, number] }
  /** Object ▸ Effects ▸ Clear Effects / Clear All Transparency, Transform ▸ Clear Transformations. */
  | { op: 'clearEffects' | 'clearAllTransparency' | 'clearTransformations'; ids: string[] }
  /** Object ▸ Content ▸ Graphic / Text / Unassigned of empty frames. */
  | { op: 'setContentType'; ids: string[]; kind: 'graphic' | 'text' | 'unassigned' }
  /** Swatch ids as documentInfo gives them (`Color/…`, `Swatch/None`). */
  | { op: 'setPaint'; ids: string[]; fill?: string; fillTint?: number; stroke?: string; strokeTint?: number; strokeWeight?: number }
  | { op: 'moveContent'; id: string; by: [number, number] }
  | { op: 'fit'; id: string; how: 'proportionally' | 'fill' | 'contentToFrame' | 'frameToContent' | 'center' };

export type EditResult = {
  document?: DocumentSummary;
  rev: number;
  label: string;
  /** Pages to draw again. */
  pages: number[];
  parts: string[];
  undo: string | null;
  redo: string | null;
  dirty: boolean;
  ms: { edit: number; rebuild: number };
  /** Objects the change made (a group, copies), in order. */
  created?: string[];
  /** Placed or pasted text: the installed fonts its characters took where their own font lacks them (server glyphfill.rs). */
  glyphFonts?: { family: string; style: string; count: number; chars: string }[];
  /** Stories left overset after the change (when the document was rebuilt). */
  overset?: number;
  /** Window ▸ History: the steps that can be undone (oldest first) and redone (next first). */
  history?: { undo: string[]; redo: string[] };
};

export class EditError extends Error {
  constructor(message: string, readonly code: string) { super(message); }
}

async function editCall(message: Record<string, unknown>): Promise<EditResult> {
  const { header } = await window.folio.request(message);
  if (!header.ok) throw new EditError(header.error ?? 'edit failed', header.code ?? 'error');
  return header as unknown as EditResult;
}

/** `selectionChanged`: the selection is not the one of the last edit (Transform Sequence Again starts a new sequence). */
/** `extra`: more fields of the request (`conveyor`: the Content Placer places a conveyor item: a Paste of it, see `conveyorCall`). */
export const editDocument = (label: string, commands: EditCommand[], selectionChanged = false, extra: Record<string, unknown> = {}) => editCall({ cmd: 'edit', label, commands, ...(selectionChanged ? { selectionChanged: true } : {}), ...extra });
/** Edit ▸ Copy of objects: the engine keeps them as they are now, for Paste. */
export async function copyObjects(ids: string[]): Promise<void> {
  const { header } = await window.folio.request({ cmd: 'copy', ids });
  if (!header.ok) throw new EditError(header.error ?? 'copy failed', header.code ?? 'error');
}
/**
 * The Content Collector / Content Placer conveyor (engine `conveyor.rs`): objects collected from documents, in the order collected.
 * It lives in the engine process, never in a document or project. Placing is `editDocument(label, [{ op: 'paste', page, at }], false, { conveyor: { item } })`.
 */
export type ConveyorObject = { id: string; kind: string; name: string };
export type ConveyorItem = { id: number; document: string; objects: ConveyorObject[] };
/** `loaded`: the Content Placer's cursor from Edit ▸ Place and Link (one set placed by the next click), outside the list. */
export type Conveyor = { items: ConveyorItem[]; current: number | null; loaded?: ConveyorItem | null };
export type ConveyorRequest =
  | { action: 'state' }
  /** One item per object; `together`: one item holding them all. */
  | { action: 'collect'; ids: string[]; together?: boolean }
  /** The item placed next: by position, or `step` places on from the current one (stops at the ends). */
  | { action: 'select'; index: number }
  | { action: 'select'; step: number }
  /** An item (default: the current one) leaves. */
  | { action: 'remove'; item?: number }
  | { action: 'clear' }
  /** Edit ▸ Place and Link: the objects, as one set, load the cursor; `unload` (Esc) empties it. Placing: `editDocument(label, [{ op: 'placeAndLink', page, at, date }], false, { conveyor: { loaded: true } })`. */
  | { action: 'load'; ids: string[] }
  | { action: 'unload' };
export async function conveyorCall(request: ConveyorRequest): Promise<Conveyor & { added?: number[] }> {
  const { header } = await window.folio.request({ cmd: 'conveyor', ...request });
  if (!header.ok) throw new EditError(header.error ?? 'conveyor failed', header.code ?? 'error');
  return header as unknown as Conveyor & { added?: number[] };
}
export const undoEdit = () => editCall({ cmd: 'undo' });
export const redoEdit = () => editCall({ cmd: 'redo' });

export type TextHit = { story: string; at: number; frame: string; page: number };
export type TextRect = { page: number; frame: string; rect: [number, number, number, number]; quad: [number, number][] };
export type TextGeometry = { caret: (TextRect & { lineStart: number; lineEnd: number; lineSelectionEnd: number }) | null; rects: TextRect[]; /** vertical text (StoryOrientation=Vertical) */ vertical?: boolean };
export type TabStop = {position:number;alignment:'LEFT_ALIGN'|'CENTER_ALIGN'|'RIGHT_ALIGN'|'CHARACTER_ALIGN';leader:string;alignmentCharacter:string};
export type TextProperties = Record<string, string | number | boolean | TabStop[]>;
export type TextFormat = Record<string, string | number | boolean | TabStop[] | null>;
export type TextFragment = { xml: string; text: string };
export const textFormat = (story: string, start: number, end: number) => textCall<{format: TextFormat; fonts: Record<string,string[]>; features: string[]}>({cmd:'textFormat',story,start,end});
/** `backward`: the search direction (Find/Change's direction buttons, Ctrl+Alt+Enter): the matches come from the end back. */
export type FindQuery={text:string;scope:'story'|'document'|'selection';story?:string;start?:number;end?:number;caseSensitive?:boolean;wholeWord?:boolean;props?:TextProperties;backward?:boolean};
export type FoundText={story:string;start:number;end:number;text:string};
export const findText=(query:FindQuery)=>textCall<{matches:FoundText[]}>({cmd:'textFind',query});
export type GrepStyle = {style:string;expression:string;source?:number};
export type NestedStyle = {style:string;delimiter:string;custom?:boolean;repetition:number;inclusive:boolean;source?:number};
export type NestedLineStyle = {style:string;lines:number;repeat?:boolean;source?:number};
export type StyleInfo = {id:string;name:string;basedOn:string;nextStyle:string;props:Record<string,string|number|boolean|null>;protected:boolean;builtIn?:boolean;grepStyles?:GrepStyle[];nestedStyles?:NestedStyle[];lineStyles?:NestedLineStyle[];shortcut?:string|null;shortcutLabel?:string|null};
/** A style that has a keyboard shortcut (the document's `styleShortcuts` list, the style that wins a key press first). */
export type StyleShortcutOwner = {kind:'paragraph'|'character';id:string;name:string;shortcut:string;label:string};
export type StyleShortcutInfo = {valid:boolean;shortcut?:string;label?:string;error?:string;owners:StyleShortcutOwner[]};
/** What a key combination (`ctrl+alt+num1`) means as a style shortcut: valid or not, its label, the styles that already use it. */
export const getStyleShortcut = (shortcut:string) => textCall<{info:StyleShortcutInfo}>({cmd:'styleShortcut',shortcut});
export const getStyleInfo = (kind:'paragraph'|'character',style:string) => textCall<{style:StyleInfo}>({cmd:'styleInfo',kind,style});
export type ObjectStyleInfo = { id:string; name:string; basedOn:string; props:Record<string,unknown>; protected:boolean };
/** The document's default object styles of new text frames and graphic frames (Object Styles panel menu). */
export const getObjectStyleDefaults = () => textCall<{style:{text:string|null;graphic:string|null}}>({cmd:'styleInfo',kind:'objectDefaults',style:''});
export const getObjectStyleInfo = (style:string) => textCall<{style:ObjectStyleInfo}>({cmd:'styleInfo',kind:'object',style});
export const textCopy = (story: string, start: number, end: number) => textCall<{fragment:TextFragment}>({cmd:'textCopy',story,start,end});
export const textNavigate = (story: string, at: number, direction: 'up' | 'down', affinity: string, x?: number) =>
  textCall<{at:number;x:number;affinity:string}>({cmd:'textNavigate',story,at,direction,affinity,x});
export type NoteContext = { footnoteTarget: string | null; endnoteTarget?: { story: string; at: number } | null; canInsertFootnoteMarker: boolean };
export type TextStory = { text: string; length: number; rev: number; overset: boolean; cell?: boolean; noteContext?: NoteContext };
/** Cell Options: absent fields stay. Swatch ids as documentInfo gives them. */
export type CellChange = { fill?: string; fillTint?: number; insets?: [number, number, number, number]; vertical?: string; rotation?: number; writingDirection?: boolean;
  strokes?: Partial<Record<'Top' | 'Left' | 'Bottom' | 'Right', { weight?: number; color?: string }>>;
  /** Cell Options ▸ Diagonal Lines (stroke style as `StrokeStyle/$ID/Dashed`, colour as a swatch id) and Graphic (clip to the cell). */
  diagonal?: DiagonalLines; clip?: boolean };
export type DiagonalLines = { topLeft?: boolean; topRight?: boolean; inFront?: boolean; weight?: number; strokeType?: string; color?: string; tint?: number };
/** The table cell under a page point: its table, place and span, and the table's size. */
export type TableHit = { story: string; frame: string; page: number; table: string; cell: [number, number]; id: string; span: [number, number];
  rows: number; columns: number; header: number; footer: number };
/** A table as the Table menu sees it (IDML values; null: the default). */
export type TableInfo = { id: string; header: number; footer: number;
  /** The table's table style (`Self`) and, per cell, its cell style. */
  style?: string | null;
  /** The alternating pattern settings (IDML attribute → text) as the table has them now. */
  options?: Record<string, string> | null;
  /** `keepWithNext`, `start`: Cell Options ▸ Keep Options (IDML Row/@KeepWithNextRow, Row/@StartRow). */
  rows: { height: number | null; minHeight: number | null; autoGrow: boolean; keepWithNext?: boolean; start?: string }[]; columns: (number | null)[];
  cells: { id: string; cell: [number, number]; span: [number, number]; style?: string | null; fill: string | null; fillTint: number | null;
    insets: (number | null)[]; vertical: string; strokes: Record<'Top' | 'Left' | 'Bottom' | 'Right', { weight: number | null; color: string | null }>;
    /** A graphic cell (Convert Cell to Graphic Cell), whether it clips its content, and its diagonal lines. */
    graphic: boolean; clip: boolean; empty: boolean; rotation: number; writingDirection: boolean;
    diagonal: { topLeft: boolean; topRight: boolean; inFront: boolean; weight: number | null; strokeType: string | null; color: string | null; tint: number | null } }[] };
export const tableHit = (page: number, x: number, y: number) => textCall<{ hit: TableHit | null }>({ cmd: 'tableHit', page, x, y });
/** A range of cells grown over merged cells, and where it shows (page rectangles). */
export const tableCells = (story: string, table: string, from: [number, number], to: [number, number]) =>
  textCall<{ from: [number, number]; to: [number, number]; rects: TextRect[] }>({ cmd: 'tableCells', story, table, from, to });
export const tableInfo = (table: string) => textCall<{ table: TableInfo }>({ cmd: 'tableInfo', table });
/** Where a table's column and row lines are on the pages (page points), for dragging them: `line` counts as `dragTableLine`'s (line k lies between
 * column/row k-1 and k, the last is the table's outer right/bottom edge), `size` is how big the column/row before it shows now. `table` is null when no table is near. */
export type TableLineSeg = { line: number; from: [number, number]; to: [number, number]; size: number; min?: number };
export type TableLines = { table: string | null; pieces: { frame: string; page: number; columns: TableLineSeg[]; rows: TableLineSeg[] }[]; columns?: number; rows?: number; header?: number };
export const tableLines = (near: { table: string } | { page: number; x: number; y: number; tol: number }) => textCall<TableLines>({ cmd: 'tableLines', ...near });
async function textCall<T>(request: Record<string, unknown>): Promise<T> {
  const { header } = await window.folio.request(request);
  if (!header.ok) throw new Error(header.error ?? 'text request failed');
  return header as unknown as T;
}
/** `story`: only that text (a story or a table cell's `Self`); else the frames' stories. */
export const textHit = (page: number, x: number, y: number, frame?: string, story?: string) =>
  textCall<{ hit: TextHit | null }>({ cmd: 'textHit', page, x, y, frame, story });
export const textStory = (story: string, at = 0) => textCall<TextStory>({ cmd: 'textStory', story, at });
export const textCursor = (story: string) => textCall({ cmd: 'textCursor', story });
export const textSelection = (story: string, start: number, end: number, affinity = 'downstream') =>
  textCall<TextGeometry>({ cmd: 'textSelection', story, start, end, affinity });

export type ProfileUse = { name: string | null; file: string | null; status: string };

export type EngineEvent = {
  event: string;
  /** The `appwin` event: the tab strip and counts of this window's group (electron/appwin.cjs). */
  view?: AppWinView;
  /** The `spell-handoff` event: Check Spelling (Search: All Documents) goes on in this window (electron/spell-docs.cjs). */
  handoff?: unknown;
  stage?: string;
  ms?: number;
  path?: string;
  detail?: unknown;
  code?: number;
  job?: string;
  done?: number;
  total?: number;
  /** package: files copied / to copy, and the PDF's state (running, done). */
  files?: number;
  filesTotal?: number;
  pdf?: string | null;
};

// Print production. Bounds are [left, top, right, bottom] in points from the page's top-left.

export type Severity = 'error' | 'warning' | 'info';

export type Finding = {
  rule: string;
  severity: Severity;
  page: number | null;
  pageName: string | null;
  item: string | null;
  message: string;
  messageId?: string;
  messageArgs?: Record<string, unknown>;
  bounds: number[] | null;
};

export type PreflightReport = {
  findings: Finding[];
  summary: { errors: number; warnings: number; infos: number; byRule: Record<string, number>; elapsedMs: number };
};

export type LinkStatus = 'found' | 'missing' | 'modified' | 'embedded' | 'ambiguous';

export type LinkRow = {
  pdf?: { page: number; crop: string; transparentBackground: boolean; width: number; height: number } | null;
  pdfPages?: number | null;
  owner: string;
  name: string;
  page: number | null;
  pageName: string | null;
  status: LinkStatus;
  /** How it was found: as recorded, beside the document, in Links, by folder or name in a search root. */
  how: string | null;
  path: string | null;
  recorded: string;
  /** Effective resolution on the page (lowest axis). */
  ppi: number | null;
  space: string | null;
  format: string | null;
  bounds: number[] | null;
  /** A content link (Content Placer ▸ Create Link), not a placed file: what `updateContentLink` / `unlinkContentLink` need. */
  content?: {
    /** The `Link` element's id (the commands' `link`). */
    link: string;
    kind: 'object' | 'story';
    source: string | null;
    /** `normal`, `outOfDate` (the source changed), `missing` (the source is gone) or `external` (the source is in another document, not read). */
    status: 'normal' | 'outOfDate' | 'missing' | 'external';
    /** A link to an object of another document. */
    external?: boolean;
    /** The placed object (story) was changed after the link was made or updated. */
    edited: boolean;
    mapStyles: boolean | null;
    /** The kind word when `name` is the general `<rectangle>`-like name (so the screen says it in its language). */
    generic: string | null;
  };
};

export type LinksReport = {
  links: LinkRow[];
  /** Content links, in rows of their own: the counts of `summary` are of the placed files. */
  contentLinks?: LinkRow[];
  summary: { links: number; found: number; missing: number; modified: number; embedded: number; ambiguous: number };
  roots: string[];
  ms: number;
};

/** PDF/X flavour (InDesign's built-in presets of the same settings). */
export type PdfPreset = 'x4' | 'x1a' | 'x3' | 'high' | 'press' | 'small';

export type ExportRequest = {
  settings?: import('./pdf/types').PdfPreset;
  path: string;
  /** Absent in settings saved by older versions: PDF/X-4. */
  preset?: PdfPreset;
  bleed: boolean;
  marks: { crop: boolean; bleed: boolean; registration: boolean; pageInfo: boolean };
  spreads: boolean;
  /** Page indexes; null = all pages. */
  pages: number[] | null;
};

export type ExportResult = {
  path: string;
  bytes: number;
  pages: number;
  ms: number;
  warnings: { kind: string; message: string; count: number; pages: number[] }[];
  fontsEmbedded: number;
  /** Fonts drawn with a substitute (the named font is not on this computer). */
  fontsSubstituted: { family: string; style: string; embedded: string | null }[];
  /** Plates: process inks and spot inks as they come out (spot, process, alias). */
  inks: { name: string; kind: string; output: string }[];
  outputIntent: string;
  /** Absent from older engines. */
  preset?: PdfPreset;
  standard?: string;
  /** Transparency flattening (PDF/X-1a, PDF/X-3). */
  flatten?: {
    pages: number;
    regions: number;
    objects: number;
    images: number;
    lineArtImages: number;
    lineArtPpi: number;
    smoothPpi: number;
    previewGraphics: number;
    bytes: number;
  } | null;
  /** RGB images and placed PDFs (their DeviceRGB) converted to the destination CMYK (PDF/X-1a). */
  converted?: { rgbImages: number; placedPdfs?: number; destination: string } | null;
};

// File ▸ Package.

/** What the window chooses; the engine decides names, fonts and refusals. */
export type PackageChoice = { links: boolean; fonts: boolean; pdf: boolean; cjkFonts: boolean };

export type PackageContact = { contact: string; company: string; phone: string; email: string };

export type PackageCounts = {
  document: { name: string; bytes: number };
  links: { placed: number; files: number; bytes: number; found: number; modified: number; missing: number; ambiguous: number; embedded: number; renamed: number; sameContent: number; rgb: number; lowRes: number; nested: number };
  fonts: { used: number; files: number; bytes: number; copied: number; notCopied: number; notSelected: number; missing: number; substituted: number; placedNotEmbedded: number };
  pdfName: string;
  bytes: number;
  ms: number;
};

/** A destination check: ok, or why not (exists: with a free name; protected; noParent). */
export type FolderState = { ok: true; warnings?: string[]; warningMessages?: EngineMessage[] } | ({ ok: false; code: string; error: string; suggestion: string | null } & EngineMessage);

export type PackagePlan = {
  counts: PackageCounts;
  preview: {
    missingLinks: { name: string; status: string; pageName: string | null }[];
    fonts: { family: string; style: string; status: string; code: string | null; reason: string | null; substitute: string | null }[];
    /** Fonts not copied, by reason code (adobeFonts, nonAdobeCjk, fsTypeRestricted, fsTypePreviewPrint, fsTypeBitmapOnly, indesignOnly, unreadable). */
    notCopiedByCode: Record<string, number>;
    renamed: { from: string; to: string }[];
    preflight: { errors: number; warnings: number; errorRules: { rule: string; title: string; count: number }[] };
  };
  folderName: string;
  pdfPossible: boolean;
  searchFolders: string[];
  dir: FolderState | null;
};

export type PackageResult = {
  dir: string;
  report: string;
  json: string;
  files: number;
  bytes: number;
  ms: number;
  timings: { planMs: number; copyMs: number; pdfMs: number; reportMs: number; finishMs: number; totalMs: number; mbPerS: number };
  document: { name: string; bytes: number; sha256: string | null };
  links: { placed: number; files: number; copied: number; missing: number; ambiguous: number; modified: number; renamed: number; failed: number; bytes: number };
  fonts: { used: number; copied: number; notCopied: number; missing: number; substituted: number; files: number };
  preflight: { errors: number; warnings: number };
  pdf:
    | { file: string; bytes: number; pages: number; ms: number; standard?: string; preset?: string; flatten?: { pages: number; lineArtPpi: number; smoothPpi: number } | null }
    | { error: string }
    | null;
  failures: { dest: string; source: string; error: string }[];
  missingLinks: string[] | null;
  fontsNotCopied: { family: string; style: string; code: string }[];
};

export type PlanRequest = PackageChoice & { dir?: string; roots: string[] };

export const packagePlan = (r: PlanRequest) => call<PackagePlan>({ cmd: 'packagePlan', ...r }, 'plan');

// Separations and overprint preview.

/** A plate of the document (after the Ink Manager's conversions and aliases). */
export type InkInfo = {
  name: string;
  kind: 'process' | 'spot';
  inkType: string;
  /** Display colour of 100 % of the ink. */
  rgb: [number, number, number];
  /** CMYK percentages the preview uses to simulate the ink. */
  cmyk: [number, number, number, number];
  /** Spot swatches printing on this plate through an alias. */
  aliases: string[];
  /** Page indexes that use the plate. */
  pages: number[];
};

export type InksReport = { inks: InkInfo[]; pages: number[][]; converted: string[] };

/** How pages are drawn: null = normal screen view. */
export type InkView =
  | { mode: 'overprint' }
  | { mode: 'separations'; inks: string[]; singlePlatesBlack?: boolean; desaturateBlack?: boolean }
  | { mode: 'inkLimit'; limit: number }
  /** Window ▸ Output ▸ Flattener Preview (features/flattener): the page ghosted with what `highlight` selects in red; `stale`: not refreshed since the last edit (the live page half way to white). */
  | { mode: 'flattener'; highlight: string; preset: string | { name: string; balance: number; strokes: boolean; text: boolean; clip: boolean }; ignoreSpreadOverrides: boolean; revision: number; stale?: boolean };

export type InkReading = { page: number; x: number; y: number; inks: { name: string; percent: number }[]; total: number };

async function call<T>(message: Record<string, unknown>, key: string): Promise<T> {
  const { header } = await window.folio.request(message);
  if (!header.ok) throw new Error(header.error ?? `${message.cmd} failed`);
  return header[key] as T;
}

/** Which page items name each swatch, and the swatches nothing names (Swatches panel: Select All Unused, Find this Color). */
export type SwatchUsage = { unused: string[]; used: Record<string, { items: string[]; other: number }> };
export const swatchUsage = async (): Promise<SwatchUsage> => {
  const { header } = await window.folio.request({ cmd: 'swatchUsage' });
  if (!header.ok) throw new Error(header.error ?? 'swatchUsage failed');
  return { unused: header.unused as string[], used: header.used as SwatchUsage['used'] };
};
export const runPreflight = () => call<PreflightReport>({ cmd: 'preflight' }, 'report');
/** Resolves every link, searching `roots` for files the document's paths do not reach. */
export const getLinks = (roots: string[]) => call<LinksReport>({ cmd: 'links', roots }, 'report');
export const exportPdf = (options: ExportRequest) => call<ExportResult>({ cmd: 'export', ...options }, 'result');

/** The document's plates; null when the engine predates separations ("unknown cmd"). */
export async function getInks(): Promise<InksReport | null> {
  const { header } = await window.folio.request({ cmd: 'inks' });
  if (!header.ok) {
    if (String(header.error ?? '').startsWith('unknown cmd')) return null;
    throw new Error(header.error ?? 'inks failed');
  }
  return header.inks as InksReport;
}

/** Ink per plate at a point of a page (points from its top left). */
export async function inkAt(page: number, x: number, y: number): Promise<InkReading> {
  const { header } = await window.folio.request({ cmd: 'inkAt', page, x, y });
  if (!header.ok) throw new Error(header.error ?? 'inkAt failed');
  return header as unknown as InkReading;
}

type Reply = { header: Record<string, any>; body: Uint8Array };

/** Kept in the app's data folder (not in documents). */
export type Settings = {
  preferences?: Record<string, import('./features/preferences/fields').PrefValue>;
  shortcuts?: Record<string, string[]>;
  flattenerPresets?: unknown;
  autoCorrectTables?: unknown;
  /** Edit ▸ Spelling ▸ User Dictionary: words added to / excluded from each language's dictionary, per language name (features/spelling). */
  userDictionaries?: unknown;
  language?: 'system' | 'ko' | 'en';
  /** Resolved by Electron, never written into a project. */
  systemLanguage?: string;
  /** Folders searched for missing links, in order. */
  linkRoots: string[];
  exportFolder: string | null;
  export: Omit<ExportRequest, 'path' | 'pages'> | null;
  /** Folder the last package was made in. */
  packageFolder: string | null;
  package: PackageChoice | null;
  packageContact: PackageContact | null;
  /** File ▸ Document Presets: the New Document settings under a name (ui/docpresets.ts cleans them). */
  documentPresets?: unknown;
  /** File ▸ Print Presets: saved print presets (the engine's dialogs `printPresets` and `printing` read and save them; the query printPresets edits a list). */
  printPresets?: unknown;
  /** Window ▸ Workspace: where the panels sit on this computer (ui/docklayout.ts DockState; the dock repairs it with normalizeDock). */
  dock?: unknown;
  /** Window ▸ Workspace: the saved workspaces of this computer, `{list, current}` (ui/workspaces.tsx repairs them). */
  workspaces?: unknown;
  /** Window ▸ Type ▸ Glyphs: the recently inserted glyphs, newest first, at most 35 (features/glyphs/panel.tsx; kept for this computer, never in a project). */
  recentGlyphs?: { family: string; style: string; id: number; cp: number | null }[];
};

declare global {
  interface Window {
    folio: {
      request(message: Record<string, unknown>): Promise<Reply>;
      /** The screen's language and the English engine sentences: the preload turns the failure texts of answers into that language (electron/engine-message.cjs). */
      setEngineLocale(language: string, english: Record<string, string>): void;
      clipboardCommand(how:'copy'|'cut'|'paste'):Promise<void>;
      openDialog(kind?: 'pdfPreset' | 'printPreset'): Promise<string | null>;
      placeDialog(): Promise<string[]>;
      /** File ▸ Import PDF Comments…: the file open window (the project folder first); the path, or null. */
      pdfCommentsDialog(root: string): Promise<string | null>;
      /** A PDF from outside the project copied into its `reviews` folder (after the person agreed): its project-relative path. */
      copyPdfIntoProject(source: string, root: string): Promise<{ ok: true; path: string } | { ok: false; error: string }>;
      initialPath(): Promise<string | null>;
      recent(): Promise<string[]>;
      pathForFile(file: File): string;
      onEvent(listener: (e: EngineEvent) => void): () => void;
      saveDialog(defaultName: string): Promise<string | null>;
      /** Asks where to save a text file (a .txt) and writes `text` there; the path, or null. */
      saveText(defaultName: string, text: string): Promise<string | null>;
      /** Asks for an Adobe Swatch Exchange file (.ase) and returns its bytes (null: cancelled). */
      swatchFileOpen(): Promise<Uint8Array | null>;
      /** Asks where to save an Adobe Swatch Exchange file (.ase) and writes `bytes`; the path, or null. */
      swatchFileSave(defaultName: string, bytes: Uint8Array): Promise<string | null>;
      chooseFolder(title?: string, defaultPath?: string): Promise<string | null>;
      reveal(file: string): Promise<void>;
      /** Opens an address in the system browser when it is http, https or mailto (Hyperlinks ▸ Go To Destination); a test run only records it. */
      openExternal(url: string): Promise<{ ok: boolean; reason?: 'empty' | 'relative' | 'scheme' | 'control'; url?: string; opened?: boolean }>;
      pathExists(file: string): Promise<boolean>;
      copyFiles(dir: string, files: string[]): Promise<number>;
      listFiles(dir: string): Promise<string[]>;
      /** Opens a folder or file with its default program; '' on success, else the error. */
      openPath(target: string): Promise<string>;
      /** Asks for a program (Edit With ▸ Other…); its path, or null (cancelled). */
      chooseProgram(): Promise<string | null>;
      /** Opens a file with a program; '' on success, else the error. */
      openWith(program: string, file: string): Promise<string>;
      /** The size, modified time and XMP packet (null: none) of a file; null when it is not there. */
      fileXmp(file: string): Promise<{ size: number; modified: number; packet: string | null } | null>;
      appInfo(): Promise<{ version: string; documents: string }>;
      /** The feature sample project, ready (electron/sample.cjs). */
      sample(): Promise<{ error: string } | { root: string; project: boolean; source: string; document: string; import: boolean; kept?: string | null }>;
      sampleImported(document: string): Promise<boolean>;
      settings(): Promise<Settings>;
      saveSettings(patch: Partial<Settings>): Promise<Settings>;
      /** A new app window with an engine session of its own; it opens `path` (a document or a project folder) when given. */
      newWindow(path?: string): Promise<boolean>;
      /** Window > Arrange across documents: a command or query of electron/appwin.cjs; the answer is `{ ok, changed, ... }` or `{ ok: false, error, message }`. */
      appWindows(cmd: { cmd: string; [arg: string]: unknown }): Promise<AppWinReply>;
      /** The app windows in the order their documents were opened (`title` null: no document), for Check Spelling > All Documents (electron/spell-docs.cjs). */
      spellWindows(): Promise<{ window: number; title: string | null; self: boolean }[]>;
      /** One read-only spelling query (`spellCheck`, `spellSuggest`) asked of another window's engine. */
      windowQuery(window: number, message: { cmd: string; [arg: string]: unknown }): Promise<{ ok: true; header: Record<string, unknown> } | { ok: false; error: string }>;
      /** The check goes on in another window: it comes to the front and gets a `spell-handoff` event carrying `payload`. */
      spellHandoff(window: number, payload: unknown): Promise<{ ok: boolean; error?: string }>;
      /** This window's tab label (null: start screen) and unsaved-changes mark, for the tab strip of the other windows. */
      appWindowMeta(meta: { title?: string | null; dirty?: boolean }): Promise<{ ok: boolean }>;
      transferSettings(mode: 'import' | 'export'): Promise<Settings | null>;
      /** The monitors' work areas (desktop pixels), for a panel group in an OS window of its own (uix-d2): absent where the app has one
       *  window only (a phone), and there a group never leaves the window. */
      displays?(): Promise<{ x: number; y: number; w: number; h: number }[]>;
      /** Where the app window's page and its panel windows' pages are on the desktop (desktop pixels, and the interface zoom), for a
       *  panel dragged between them (uix-d3, ui/dockwindows.ts); absent with one window only. */
      windowPlaces?(): Promise<import('./ui/dockwindows').WindowPlaces | null>;
      /** A panel window (by its name) made see-through (`opacity`) or moved (`bounds`, desktop pixels). */
      popoutWindow?(name: string, how: { opacity?: number; bounds?: { x: number; y: number; w: number; h: number } }): Promise<boolean>;
    };
  }
}

/** What a window is told about the app windows it sits with (electron/appwin.cjs `view`): its group's tab strip and the counts. */
export type AppWinView = {
  doc: number;
  group: number;
  /** 'tabs', or the id of the arrangement the group's windows are tiled in. */
  mode: string;
  tabs: { id: number; title: string | null; dirty: boolean; active: boolean }[];
  /** Windows in all, and groups (frames) in all. */
  docs: number;
  groups: number;
};
export type AppWinReply = { ok: true; changed: boolean; view?: AppWinView; state?: unknown; layout?: unknown[] } | { ok: false; error: string; message: string };

export async function openDocument(path: string): Promise<DocumentSummary> {
  const { header } = await window.folio.request({ cmd: 'open', path });
  if (!header.ok) throw new Error(header.error ?? 'open failed');
  return header.document as DocumentSummary;
}

export type Bitmap = { page: number; scale: number; width: number; height: number; pixels: Uint8ClampedArray<ArrayBuffer>; ms: number };

type Job = { page: number; scale: number; region?: number[]; view?: InkView | null; priority: number; resolve: (b: Bitmap) => void; reject: (e: Error) => void };

/** Limits renders in flight and serves visible pages before thumbnails. */
export class RenderQueue {
  private queue: Job[] = [];
  private running = 0;
  /** Display Performance of the window (View ▸ Display Performance): sent with every render; the engine's default is High Quality. */
  display: { view: 'OPTIMIZED' | 'TYPICAL' | 'HIGH_QUALITY'; ignoreLocal: boolean } = { view: 'HIGH_QUALITY', ignoreLocal: false };
  constructor(private readonly limit = 4) {}

  render(page: number, scale: number, priority: number, view?: InkView | null): Promise<Bitmap> {
    return new Promise((resolve, reject) => {
      this.queue.push({ page, scale, view, priority, resolve, reject });
      this.queue.sort((a, b) => a.priority - b.priority);
      this.pump();
    });
  }

  /** Renders only `region` = [left, top, right, bottom] of the page, in points. */
  renderRegion(page: number, scale: number, region: number[], priority: number, view?: InkView | null): Promise<Bitmap> {
    return new Promise((resolve, reject) => {
      this.queue.push({ page, scale, region, view, priority, resolve, reject });
      this.queue.sort((a, b) => a.priority - b.priority);
      this.pump();
    });
  }

  clear() {
    for (const j of this.queue) j.reject(new Error('cancelled'));
    this.queue = [];
  }

  private pump() {
    while (this.running < this.limit && this.queue.length) {
      const job = this.queue.shift()!;
      this.running++;
      window.folio
        .request({ cmd: 'render', page: job.page, scale: job.scale, ...(job.region ? { region: job.region } : {}), ...(job.view ? { view: job.view } : {}), display: this.display })
        .then(({ header, body }) => {
          if (!header.ok) throw new Error(header.error ?? 'render failed');
          job.resolve({
            page: job.page,
            scale: job.scale,
            width: header.width,
            height: header.height,
            pixels: new Uint8ClampedArray(body.buffer as ArrayBuffer, body.byteOffset, body.byteLength),
            ms: header.stats?.ms ?? 0,
          });
        })
        .catch((e) => job.reject(e))
        .finally(() => {
          this.running--;
          this.pump();
        });
    }
  }
}
