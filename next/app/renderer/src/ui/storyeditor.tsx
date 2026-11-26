// Edit ▸ Edit in Story Editor (Ctrl+Y): the story as plain text in a window of its own, with InDesign's Story Editor view
// options (View ▸ Story Editor) and display preferences (Preferences ▸ Story Editor Display). Every change is an engine
// edit (insertText / deleteText); the window shows what the engine holds.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { Dropdown } from './menu';
import { CONTEXT_MENUS } from './menutree';
import { textFormat, textStory } from '../engine';
import type { EditCommand, EditResult } from '../engine';
import type { Command, Commands } from './commands';
import type { EditorialStory } from '../features/editorial/types';
import './storyeditor.css';

type Edit = (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
type Reveal = { story: string; start: number; end: number };
type Context = {
  documentPath?: string;
  editable: boolean;
  /** The story Edit in Story Editor opens: the one with the text cursor, else the one of the selected text frame. */
  story: string | null;
  selection: { story: string; start: number; end: number } | null;
  info: { paragraphStyles?: { id: string; name: string }[]; editorial?: EditorialStory[] } | null;
  prefs: Record<string, unknown> | undefined;
  setPref: (key: string, value: boolean) => void;
  edit: Edit;
  undo: () => void;
  redo: () => void;
  reveal: (to: Reveal) => void;
  /** The commands as the menus see them now (the table icon's menu is the text context menu). */
  commands?: () => Commands;
};
/** A table of the story and its cells in the order the Story Editor lists them (the engine's `storyTables`). */
type StoryTable = { id: string; order: 'rows' | 'columns'; rows: number; columns: number; cells: { id: string; row: number; column: number; group: number; label: number; first: boolean }[] };

/** Preference keys: InDesign's own (Story Editor Display) and Folio's for what InDesign keeps in the View menu. */
const STYLES = 'galleyPreferences.showParagraphStyleNames', DEPTH = 'galleyPreferences.showDepthRuler', MARKS = 'galleyPreferences.showParagraphBreakMarks';
const CHANGES = 'folio.storyEditor.showChanges', COLLAPSED = 'folio.storyEditor.footnotesCollapsed';

export function useStoryEditor(ctx: Context) {
  const [open, setOpen] = useState<Reveal | null>(null);
  useEffect(() => setOpen(null), [ctx.documentPath]);
  const on = (key: string, fallback: boolean) => (ctx.prefs?.[key] === undefined ? fallback : ctx.prefs[key] !== false);
  const [styles, depth, marks, changes, collapsed] = [on(STYLES, true), on(DEPTH, true), on(MARKS, false), on(CHANGES, true), ctx.prefs?.[COLLAPSED] === true];
  // The View ▸ Story Editor items work in the Story Editor only (grey elsewhere, as in InDesign).
  const toggle = (key: string, now: boolean) => (open ? () => ctx.setPref(key, !now) : undefined);
  // The table icon's menu (Arrange by rows / columns): the items work for the table whose icon was clicked, grey elsewhere.
  const [tableMenu, setTableMenu] = useState<{ id: string; order: 'rows' | 'columns' } | null>(null);
  const [tableRev, setTableRev] = useState(0);
  const arrange = (cmd: 'arrangeByRows' | 'arrangeByColumns') => (tableMenu && ctx.editable
    ? () => { void window.folio.request({ cmd, table: tableMenu.id }).then(({ header }) => { if (header.ok) setTableRev(r => r + 1); else console.error('arrange', header.error); }); }
    : undefined);
  const handlers: Record<string, Partial<Command>> = {
    'edit.storyEditor': { run: open ? () => setOpen(null) : ctx.story ? () => setOpen({ story: ctx.story!, start: ctx.selection?.story === ctx.story ? ctx.selection.start : 0, end: ctx.selection?.story === ctx.story ? ctx.selection.end : 0 }) : undefined, checked: !!open },
    'id.view.storyEditor.showChanges': { run: toggle(CHANGES, changes), checked: changes },
    'id.view.storyEditor.showStyleNameColumn': { run: toggle(STYLES, styles), checked: styles },
    'id.view.storyEditor.showDepthRuler': { run: toggle(DEPTH, depth), checked: depth },
    'id.view.storyEditor.showParagraphBreakMarks': { run: toggle(MARKS, marks), checked: marks },
    'id.view.storyEditor.collapseAllFootnotes': { label: collapsed ? tr('모든 각주 확장') : undefined, run: toggle(COLLAPSED, collapsed) },
    'id.ctx.text.arrangeByRows': { run: arrange('arrangeByRows'), checked: tableMenu?.order === 'rows' },
    'id.ctx.text.arrangeByColumns': { run: arrange('arrangeByColumns'), checked: tableMenu?.order === 'columns' },
  };
  const close = useCallback((to?: Reveal) => { setOpen(null); if (to) ctx.reveal(to); }, [ctx.reveal]);
  useEffect(() => { if (!open) setTableMenu(null); }, [open]);
  const view = { styles, depth, marks, changes, collapsed, prefs: ctx.prefs, tableRev, openTableMenu: setTableMenu };
  return { handlers, open: !!open, dialog: open && <StoryEditorWindow key={open.story} start={open} ctx={ctx} view={view} close={close} /> };
}

type View = { styles: boolean; depth: boolean; marks: boolean; changes: boolean; collapsed: boolean; prefs: Record<string, unknown> | undefined;
  /** Counts the changes of a table's order; the table whose icon menu is open (null: none). */
  tableRev: number; openTableMenu: (t: { id: string; order: 'rows' | 'columns' } | null) => void };

/** What changed between two texts, as a delete and an insert (code points, as the engine counts). */
export function textDiff(before: string, after: string) {
  const a = Array.from(before), b = Array.from(after);
  let s = 0;
  while (s < a.length && s < b.length && a[s] === b[s]) s++;
  let ea = a.length, eb = b.length;
  while (ea > s && eb > s && a[ea - 1] === b[eb - 1]) { ea--; eb--; }
  return { start: s, end: ea, text: b.slice(s, eb).join('') };
}
const toCodePoints = (s: string, unit: number) => Array.from(s.slice(0, unit)).length;
const toUnits = (s: string, cp: number) => { let u = 0, n = 0; for (const ch of s) { if (n >= cp) break; u += ch.length; n++; } return u; };

/** The text of a story in a textarea, written back to the engine as the typing settles (and on demand). */
/** `lead`: the start of the engine's text that is not shown or edited (a footnote's number mark and tab). */
function useStoryText(story: string, edit: Edit, changed: () => void, lead = '') {
  const [value, setValue] = useState<string | null>(null);
  const hidden = useRef(''), engine = useRef(''), current = useRef(''), timer = useRef(0), queue = useRef<Promise<unknown>>(Promise.resolve()), alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; window.clearTimeout(timer.current); }; }, []);
  const load = useCallback(async () => {
    const t = await textStory(story);
    if (!alive.current) return t.text;
    hidden.current = lead && t.text.startsWith(lead) ? lead : '';
    engine.current = t.text; current.current = t.text.slice(hidden.current.length).replace(/\r/g, '\n'); setValue(current.current);
    return t.text;
  }, [story, lead]);
  useEffect(() => { void load().catch(() => {}); }, [load]);
  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    queue.current = queue.current.then(async () => {
      const next = hidden.current + current.current.replace(/\n/g, '\r');
      if (next === engine.current) return;
      const d = textDiff(engine.current, next);
      const commands: EditCommand[] = [];
      if (d.end > d.start) commands.push({ op: 'deleteText', story, start: d.start, end: d.end });
      if (d.text) commands.push({ op: 'insertText', story, at: d.start, text: d.text });
      const done = await edit(tr('스토리 편집기 입력'), commands).catch(() => null);
      if (!done) { await load(); return; }
      engine.current = next;
      // the engine may have changed what it was given (a paragraph break takes its style): show what it holds
      const held = (await textStory(story)).text;
      if (held !== next) await load();
      changed();
    }).catch(() => {});
    return queue.current;
  }, [story, edit, load, changed]);
  const type = useCallback((v: string) => {
    current.current = v; setValue(v);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { void flush(); }, 400);
  }, [flush]);
  return { value, type, flush, load, text: () => hidden.current + current.current.replace(/\n/g, '\r') };
}

export const UI_COLORS: Record<string, string> = {
  WHITE: '#ffffff', BLACK: '#000000', RED: '#ff0000', GREEN: '#00a651', BLUE: '#0000ff', YELLOW: '#ffff00', MAGENTA: '#ff00ff', CYAN: '#00ffff', GRAY: '#808080',
  LIGHT_BLUE: '#4f9fff', ORANGE: '#ff8000', DARK_GREEN: '#006400', TEAL: '#008080', TAN: '#d2b48c', BROWN: '#8b4513', VIOLET: '#8a2be2', GOLD: '#ffd700',
  DARK_BLUE: '#00008b', PINK: '#ffc0cb', LAVENDER: '#e6e6fa', BRICK_RED: '#b22222', OLIVE_GREEN: '#6b8e23', PEACH: '#ffdab9', BURGUNDY: '#800020',
  GRASS_GREEN: '#7cfc00', OCHRE: '#cc7722', PURPLE: '#800080', LIGHT_GRAY: '#d3d3d3', CHARCOAL: '#36454f', GRID_BLUE: '#3399ff', GRID_ORANGE: '#ff9933',
  FIESTA: '#ff6347', LIGHT_OLIVE: '#bdb76b', LIPSTICK: '#c71585', CUTE_TEAL: '#40e0d0', SULPHUR: '#e4d00a', GRID_GREEN: '#33cc66',
};
const SPACING: Record<string, number> = { SINGLE_SPACE: 1.25, ONE_AND_HALF_SPACE: 1.9, DOUBLE_SPACE: 2.5, TRIPLE_SPACE: 3.75 };

type Para = { start: number; end: number; text: string };
function paragraphsOf(text: string): Para[] {
  const out: Para[] = [];
  let at = 0;
  for (const p of text.split('\r')) { const n = Array.from(p).length; out.push({ start: at, end: at + n, text: p }); at += n + 1; }
  return out;
}

function StoryEditorWindow({ start, ctx, view, close }: { start: Reveal; ctx: Context; view: View; close: (to?: Reveal) => void }) {
  const story = start.story;
  // Preferences ▸ Type ▸ Drag and Drop Text Editing ▸ Enable in Story Editor (on unless false; a user setting): off, the text box takes no drag
  const [dragDrop, setDragDrop] = useState(true);
  useEffect(() => { void window.folio.settings().then((s) => setDragDrop(s.preferences?.['textEditingPreferences.allowDragAndDropTextInStory'] !== false)).catch(() => undefined); }, []);
  const noDrag = dragDrop ? undefined : (e: React.DragEvent) => e.preventDefault();
  const [revision, setRevision] = useState(0);
  const changed = useCallback(() => setRevision(r => r + 1), []);
  const main = useStoryText(story, ctx.edit, changed);
  const area = useRef<HTMLTextAreaElement>(null), mirror = useRef<HTMLDivElement>(null);
  const [layout, setLayout] = useState<{ paragraph: { top: number; height: number; mark: [number, number] }[]; height: number }>({ paragraph: [], height: 0 });
  const [names, setNames] = useState<string[]>([]);
  const [depths, setDepths] = useState<{ depth: number; frameBreak: boolean }[]>([]);
  const [notes, setNotes] = useState<{ at: number; story: string }[]>([]);
  const [folded, setFolded] = useState<Record<string, boolean>>({});
  const [tables, setTables] = useState<StoryTable[]>([]);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const text = main.value === null ? '' : main.value.replace(/\n/g, '\r');
  const paragraphs = useMemo(() => paragraphsOf(text), [text]);
  const prefs = view.prefs ?? {};
  const color = (key: string, fallback: string) => UI_COLORS[String(prefs[key] ?? '')] ?? fallback;
  const spacing = SPACING[String(prefs['galleyPreferences.lineSpacingValue'] ?? 'SINGLE_SPACE')] ?? 1.25;
  const style = { background: color('galleyPreferences.backgroundColor', '#ffffff'), color: color('galleyPreferences.textColor', '#000000'), lineHeight: spacing } as React.CSSProperties;

  // the caret where the layout had it
  const placed = useRef(false);
  useLayoutEffect(() => {
    if (main.value === null || placed.current || !area.current) return;
    placed.current = true;
    const a = area.current, v = main.value;
    a.focus();
    a.setSelectionRange(toUnits(v, start.start), toUnits(v, start.end));
  }, [main.value, start.start, start.end]);

  // paragraph boxes, measured in a copy of the text set the same way
  useLayoutEffect(() => {
    const m = mirror.current;
    if (!m) return;
    const top = m.getBoundingClientRect().top;
    const rows = Array.from(m.children).map(el => {
      const e = el as HTMLElement, r = document.createRange();
      r.selectNodeContents(e);
      const rects = Array.from(r.getClientRects());
      const last = rects[rects.length - 1];
      const box = m.getBoundingClientRect();
      return { top: e.offsetTop, height: e.offsetHeight, mark: [last ? last.right - box.left : 0, last ? last.top - top : e.offsetTop] as [number, number] };
    });
    setLayout({ paragraph: rows, height: m.scrollHeight });
  }, [text, view.styles, view.depth, view.marks, view.changes, spacing]);

  // style names, depths and footnotes of the engine's story
  useEffect(() => {
    const value = main.value;
    if (value === null) return;
    let alive = true;
    const ps = paragraphsOf(value.replace(/\n/g, '\r'));
    void (async () => {
      const names: string[] = [];
      for (let i = 0; i < ps.length && alive; i += 24) {
        const part = await Promise.all(ps.slice(i, i + 24).map(p => textFormat(story, p.start, p.start).then(r => String(r.format.paragraphStyle ?? '')).catch(() => '')));
        names.push(...part);
      }
      if (!alive) return;
      const known = new Map((ctx.info?.paragraphStyles ?? []).map(s => [s.id, s.name]));
      setNames(names.map(id => known.get(id) ?? id.replace(/^ParagraphStyle\//, '').replace(/^\$ID\//, '')));
      const frames = ((await window.folio.request({ cmd: 'textLayout', story })).header.frames ?? []) as { lines: { start: number; end: number; baseline: number }[] }[];
      let offset = 0;
      const rows: { depth: number; frameBreak: boolean }[] = ps.map(() => ({ depth: 0, frameBreak: false }));
      frames.filter(f => f.lines.length).forEach((f, k) => {
        const first = f.lines[0].baseline, last = f.lines[f.lines.length - 1].baseline;
        const step = f.lines.length > 1 ? (last - first) / (f.lines.length - 1) : 0;
        ps.forEach((p, i) => {
          const line = f.lines.find(l => l.start <= p.start && p.start < Math.max(l.end, l.start + 1));
          if (line) { rows[i] = { depth: offset + line.baseline - first, frameBreak: k > 0 && f.lines[0].start === p.start }; }
        });
        offset += last - first + step;
      });
      if (alive) setDepths(rows);
      // the k-th footnote reference of the story is the footnote story `<story>/footnote/<k>`
      const refs: { at: number; story: string }[] = [];
      let n = 0, k = 0;
      for (const ch of Array.from(value.replace(/\n/g, '\r'))) {
        if (ch === '\u0004') {
          const target = (await textStory(story, n)).noteContext?.footnoteTarget ?? `${story}/footnote/${k}`;
          if (await textStory(target).then(() => true, () => false)) refs.push({ at: n, story: target });
          k++;
        }
        n++;
      }
      if (alive) setNotes(refs);
    })().catch(() => {});
    return () => { alive = false; };
    // Re-measured when the text, or the document behind it, changed
  }, [main.value, revision, story, ctx.info?.paragraphStyles]);

  // the tables of the story, their cells in the order of each table's view
  useEffect(() => {
    let alive = true;
    void window.folio.request({ cmd: 'storyTables', story }).then(({ header }) => { if (alive && header.ok) setTables(header.tables as StoryTable[]); }).catch(() => {});
    return () => { alive = false; };
  }, [story, revision, main.value, view.tableRev]);
  const tableMenu = (e: React.MouseEvent, t: StoryTable) => {
    e.preventDefault(); e.stopPropagation();
    view.openTableMenu({ id: t.id, order: t.order });
    setMenuAt({ x: e.clientX, y: e.clientY });
  };
  const closeTableMenu = () => { setMenuAt(null); view.openTableMenu(null); };

  const stop = useCallback(async (to: 'close' | 'undo' | 'redo') => {
    await main.flush();
    if (to === 'close') {
      const a = area.current, v = main.value ?? '';
      close(a ? { story, start: toCodePoints(v, a.selectionStart), end: toCodePoints(v, a.selectionEnd) } : undefined);
      return;
    }
    (to === 'undo' ? ctx.undo : ctx.redo)();
    window.setTimeout(() => { void main.load().then(changed); }, 120);
  }, [main, close, story, ctx.undo, ctx.redo, changed]);

  const key = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase();
    if (e.key === 'Escape') { e.preventDefault(); void stop('close'); }
    else if (mod && k === 'y') { e.preventDefault(); void stop('close'); }
    else if (mod && k === 'z') { e.preventDefault(); void stop(e.shiftKey ? 'redo' : 'undo'); }
  };

  const story_ = ctx.info?.editorial?.find(s => s.id === story);
  const changes = view.changes ? (story_?.changes ?? []) : [];
  const title = paragraphs.find(p => p.text.trim())?.text.trim().slice(0, 40) ?? '';
  const noteFold = (id: string) => folded[id] ?? view.collapsed;
  const gutter = (view.depth ? 1 : 0) + (view.styles ? 1 : 0);

  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('스토리 편집기')} onPointerDown={e => e.stopPropagation()} onKeyDown={key}>
    <div className="card dialog story-editor">
      <header><h2>{tr('스토리 편집기')}{title && <span> — {title}</span>}</h2><button type="button" onClick={() => void stop('close')}>{tr('닫기')}</button></header>
      <div className="se-scroll" data-gutters={gutter}>
        <div className="se-body" style={style}>
          {view.depth && <div className="se-depth" aria-label={tr('깊이 눈금자')}>{layout.paragraph.map((r, i) => <span key={i} className={depths[i]?.frameBreak ? 'frame-break' : ''} style={{ top: r.top }}>{Math.round(depths[i]?.depth ?? 0)}</span>)}</div>}
          {view.styles && <div className="se-styles" aria-label={tr('단락 스타일 이름')}>{layout.paragraph.map((r, i) => <span key={i} style={{ top: r.top }}>{names[i] ?? ''}</span>)}</div>}
          <div className="se-text" style={{ height: layout.height || undefined }}>
            <div className="se-mirror se-changes" ref={mirror} aria-hidden>
              {paragraphs.map((p, i) => <div key={i}>{marked(p, changes) || '​'}</div>)}
            </div>
            <textarea ref={area} className="se-area" aria-label={tr('스토리 텍스트')} spellCheck={false} readOnly={!ctx.editable}
              value={main.value ?? ''} onChange={e => main.type(e.target.value)} onBlur={() => void main.flush()} onDragStart={noDrag} onDrop={noDrag} data-drag-drop={dragDrop ? 'on' : 'off'} />
            {view.marks && layout.paragraph.map((r, i) => <i key={i} className="se-mark" style={{ left: r.mark[0] + 2, top: r.mark[1] }}>¶</i>)}
          </div>
        </div>
        {tables.length > 0 && <div className="se-tables"><p className="p-note" data-note="storyTables" style={{ color: '#eee', margin: '0 8px 4px 0' }}>{tr('표는 글 아래에 모아 보입니다.')}</p>{tables.map(t => <StoryTableView key={t.id} table={t} ctx={ctx} changed={changed} style={style} onMenu={e => tableMenu(e, t)} />)}</div>}
        {notes.length > 0 && <div className="se-notes">{notes.map((n, i) => <Footnote key={n.story} n={i + 1} story={n.story} ctx={ctx} folded={noteFold(n.story)} toggle={() => setFolded(f => ({ ...f, [n.story]: !noteFold(n.story) }))} changed={changed} style={style} />)}</div>}
      </div>
    </div>
    {menuAt && <Dropdown items={CONTEXT_MENUS.text ?? []} commands={ctx.commands?.() ?? {}} x={menuAt.x} y={menuAt.y} onClose={closeTableMenu} />}
  </div>, document.body);
}

/** A table of the story: its icon (right-click: the text menu with Arrange by rows / Arrange by columns) and its cells one under the other,
 *  under a dashed divider per row or per column (InDesign: "Row 1", and "Column 1" on every column divider). */
function StoryTableView({ table, ctx, changed, style, onMenu }: { table: StoryTable; ctx: Context; changed: () => void; style: React.CSSProperties; onMenu: (e: React.MouseEvent) => void }) {
  return <section className="se-table" style={style} data-order={table.order} aria-label={tr('표')}>
    <div className="se-table-head" onContextMenu={onMenu}>
      <span aria-hidden>▾</span>
      <svg className="se-table-icon" viewBox="0 0 24 12" width="24" height="12" role="img" aria-label={tr('표 아이콘')}><rect x=".5" y=".5" width="23" height="11" fill="none" stroke="currentColor" /><path d="M8 0v12M16 0v12M0 6h24" stroke="currentColor" /></svg>
    </div>
    {table.cells.map(c => <div key={c.id} className="se-cell-row">
      {c.first && <div className="se-divider" data-group={c.group}><span>{table.order === 'rows' ? tr('행 {0}', { 0: c.label }) : tr('열 {0}', { 0: c.label })}</span></div>}
      <StoryCell id={c.id} ctx={ctx} changed={changed} />
    </div>)}
  </section>;
}

function StoryCell({ id, ctx, changed }: { id: string; ctx: Context; changed: () => void }) {
  const cell = useStoryText(id, ctx.edit, changed);
  return <textarea className="se-cell" data-cell={id} aria-label={tr('표 칸')} spellCheck={false} readOnly={!ctx.editable} rows={Math.max(1, (cell.value ?? '').split('\n').length)}
    value={cell.value ?? ''} onChange={e => cell.type(e.target.value)} onBlur={() => void cell.flush()} />;
}

/** One paragraph with the tracked changes in it: inserted text underlined, deleted text struck out, moved text marked. */
function marked(p: Para, changes: { start: number; end: number; change_type: string }[]) {
  const cut = Array.from(p.text);
  const hits = changes.filter(c => c.start < p.end && c.end > p.start).sort((a, b) => a.start - b.start);
  if (!hits.length) return p.text;
  const out: React.ReactNode[] = [];
  let at = 0;
  hits.forEach((c, k) => {
    const from = Math.max(c.start - p.start, at), to = Math.min(c.end - p.start, cut.length);
    if (to <= from) return;
    if (from > at) out.push(cut.slice(at, from).join(''));
    const kind = /delet/i.test(c.change_type) ? 'deleted' : /mov/i.test(c.change_type) ? 'moved' : 'inserted';
    out.push(<span key={k} className={`se-change ${kind}`}>{cut.slice(from, to).join('')}</span>);
    at = to;
  });
  if (at < cut.length) out.push(cut.slice(at).join(''));
  return out;
}

function Footnote({ n, story, ctx, folded, toggle, changed, style }: { n: number; story: string; ctx: Context; folded: boolean; toggle: () => void; changed: () => void; style: React.CSSProperties }) {
  const note = useStoryText(story, ctx.edit, changed, '￼\t');
  return <section className="se-note" style={style}>
    <button type="button" className="se-note-head" aria-expanded={!folded} onClick={toggle}>{folded ? '▸' : '▾'} {tr('각주 {0}', { 0: n })}</button>
    {!folded && <textarea aria-label={tr('각주 {0}', { 0: n })} spellCheck={false} readOnly={!ctx.editable} rows={Math.max(1, (note.value ?? '').split('\n').length)}
      value={note.value ?? ''} onChange={e => note.type(e.target.value)} onBlur={() => void note.flush()} />}
  </section>;
}
