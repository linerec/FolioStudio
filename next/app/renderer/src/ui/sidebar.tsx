import { SampleCards } from './SampleCards';
// The Folio workspace's two views (user decision 2026-11-20, CLAUDE.md "화면도 프로젝트 중심이다"): the explorer of the project folder
// (its documents, books, libraries, pictures, fonts and other files, with the open documents first) and the tools with the fill and
// stroke (the same ToolsPanel as InDesign Classic's left edge). Since uix-d2 they are dock panels like the others (ui/dock.tsx: they
// start in the left area, the explorer above the tools, with the activity bar beside it; they float, join tabs and fold like any
// panel), and where they sit is the dock layout in the engine's user settings. The tree is the engine's (`projectInfo` →
// folio-project tree.rs, with each file's `resource` kind); opening, making and importing are the commands the menus use.
import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { tr } from '../i18n';
import { Icon } from './icons';
import { PopupMenu } from './menu';
import type { Commands } from './commands';
import type { MenuEntry } from './menudef';
import { NameDialog } from './tablestylegroups';
import type { ProjectInfo, ProjectTree, RecentProject, TreeEntry } from '../project';
import './sidebar.css';

type Kind = 'dir' | 'document' | 'book' | 'library' | 'image' | 'font' | 'data' | 'indesign' | 'pdf' | 'file';
type Entry = TreeEntry & { resource?: string };
const kindOf = (e: Entry): Kind => (e.kind === 'dir' ? 'dir' : e.kind === 'document' ? 'document' : ((e.resource as Kind | undefined) ?? 'file'));
const ICON: Record<Kind, string> = { dir: 'folder', document: 'document', book: 'book', library: 'library', image: 'image', font: 'fontFile', data: 'dataFile', indesign: 'indesignFile', pdf: 'file', file: 'file' };
const KIND_NAME: Record<Kind, () => string> = {
  dir: () => tr('폴더'), document: () => tr('Folio 문서'), book: () => tr('책'), library: () => tr('라이브러리'), image: () => tr('그림'),
  font: () => tr('글꼴'), data: () => tr('데이터 파일'), indesign: () => tr('InDesign 문서'), pdf: () => tr('PDF'), file: () => tr('파일'),
};
const fold = (s: string) => s.normalize('NFC').toLocaleLowerCase();
const parentOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '');

export type ExplorerActions = {
  /** A Folio document (project-relative): its window. */
  open: (rel: string) => void;
  openBook: (rel: string) => void;
  openLibrary: (rel: string) => void;
  /** A picture or PDF (whole path) into the loaded place cursor; null while no document can take it (with the reason). */
  place: ((file: string) => void) | null;
  placeWhy: string;
  /** INDD, INDT, IDML (whole path), or the file window when absent. */
  importDoc: (file?: string) => void;
  reveal: (file: string) => void;
  refresh: () => void;
  newDocument: () => void;
  /** File ▸ New ▸ Book / Library in a folder of the project ('' the project folder). */
  newBook: (folder: string) => void;
  newLibrary: (folder: string) => void;
  /** The engine's projectNewFolder: answers the new folder's project-relative path, or throws the reason in the user's words. */
  newFolder: (folder: string, name: string) => Promise<string>;
  newProject: () => void;
  openProject: () => void;
  openRecent: (root: string) => void;
  note: (text: string) => void;
  path: (rel: string) => string;
};

type OpenDoc = { id: number; title: string; dirty: boolean; active: boolean };

type Props = {
  project: { info: ProjectInfo; tree: ProjectTree } | null;
  recentProjects: RecentProject[];
  /** The document open in this window: project-relative path (null without a project), its pages and unsaved mark. */
  current: { rel: string | null; name: string; pages: number; dirty: boolean; readOnly: boolean } | null;
  /** The documents of the other app windows (their tabs), when more than one is open. */
  openDocs: OpenDoc[] | null;
  onActivateDoc: (id: number) => void;
  actions: ExplorerActions;
};

/** The explorer view (a dock panel since uix-d2: it docks, floats, joins other panels' tabs and folds like them). The class `sb-explorer`
 *  is the one the checks and the styles know. */
export function ExplorerView(p: Props) {
  return (
    <section className="sb-tier sb-explorer" aria-label={tr('탐색기')}>
      <Explorer {...p} />
    </section>
  );
}

/** The tools view: the Tools panel's buttons and the fill and stroke (`tools`), in the sidebar's grid, or (floating) in the rows or
 *  columns a person chose; under them (`help`, the grid only) what the chosen tool does (ui/toolhelpview.tsx). */
export function ToolboxView({ tools, help }: { tools: ReactNode; help?: ReactNode }) {
  return (
    <section className="sb-tier sb-tools" aria-label={tr('도구')}>
      <div className="sb-tools-body">{tools}</div>
      {help}
    </section>
  );
}

type Row = { entry: Entry; kind: Kind; depth: number; open: boolean; hasKids: boolean };

function Explorer(p: Props) {
  const { project, actions } = p;
  const tree = project?.tree;
  const docs = useMemo(() => new Map((tree?.documents ?? []).map((d) => [d.path, d])), [tree]);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [filter, setFilter] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [openGroup, setOpenGroup] = useState(true);
  const [menu, setMenu] = useState<{ x: number; y: number; row: Row | null } | null>(null);
  // New Folder…: the folder it goes in ('' the project folder) while its name is asked
  const [folderIn, setFolderIn] = useState<string | null>(null);
  const treeRef = useRef<HTMLDivElement>(null);
  useEffect(() => { setOpen({}); setSelected(null); setFilter(''); }, [project?.info.root]);

  const q = fold(filter.trim());
  // the rows to draw: folders first (the engine's order), a folder's children under it while it is open; with a filter, the names that
  // contain it and the folders on their way (opened)
  const rows = useMemo(() => {
    const out: Row[] = [];
    if (!tree) return out;
    const hits = (e: Entry): boolean => fold(e.name).includes(q) || e.children.some((c) => hits(c as Entry));
    const walk = (list: Entry[], depth: number) => {
      for (const e of list) {
        if (q && !hits(e)) continue;
        const kind = kindOf(e);
        const isOpen = kind === 'dir' && (q ? true : (open[e.path] ?? depth === 0));
        out.push({ entry: e, kind, depth, open: isOpen, hasKids: e.children.length > 0 });
        if (isOpen) walk(e.children as Entry[], depth + 1);
      }
    };
    walk(tree.entries as Entry[], 0);
    return out;
  }, [tree, open, q]);
  const index = useMemo(() => new Map(rows.map((r, i) => [r.entry.path, i])), [rows]);
  const allDirs = useMemo(() => {
    const out: string[] = [];
    const walk = (list: TreeEntry[]) => { for (const e of list) if (e.kind === 'dir') { out.push(e.path); walk(e.children); } };
    walk(tree?.entries ?? []);
    return out;
  }, [tree]);
  const counts = useMemo(() => {
    let files = 0;
    const walk = (list: TreeEntry[]) => { for (const e of list) { if (e.kind === 'dir') walk(e.children); else files++; } };
    walk(tree?.entries ?? []);
    return { files, documents: tree?.documents.length ?? 0 };
  }, [tree]);

  const folderOf = (row: Row | null) => (!row ? '' : row.kind === 'dir' ? row.entry.path : parentOf(row.entry.path));
  const toggle = (path: string, to?: boolean) => setOpen((o) => ({ ...o, [path]: to ?? !(o[path] ?? !path.includes('/')) }));
  /** What opening a row does: a document is its window, a book or library its window, a picture the place cursor, an InDesign file the import. */
  const activate = useCallback((row: Row) => {
    const rel = row.entry.path;
    switch (row.kind) {
      case 'dir': toggle(rel); return;
      case 'document': actions.open(rel); return;
      case 'book': actions.openBook(rel); return;
      case 'library': actions.openLibrary(rel); return;
      case 'image': case 'pdf': if (actions.place) actions.place(actions.path(rel)); else actions.note(actions.placeWhy); return;
      case 'indesign': actions.importDoc(actions.path(rel)); return;
      default: actions.reveal(actions.path(rel));
    }
  }, [actions]);  // eslint-disable-line react-hooks/exhaustive-deps

  const sel = selected !== null && index.has(selected) ? selected : null;
  const reveal = (path: string) => window.setTimeout(() => treeRef.current?.querySelector(`[data-path="${CSS.escape(path)}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' }), 0);
  const go = (i: number) => { const r = rows[Math.max(0, Math.min(rows.length - 1, i))]; if (r) { setSelected(r.entry.path); reveal(r.entry.path); } };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const i = sel === null ? -1 : index.get(sel)!;
    const row = i >= 0 ? rows[i] : null;
    let handled = true;
    switch (e.key) {
      case 'ArrowDown': go(i + 1); break;
      case 'ArrowUp': go(i < 0 ? rows.length - 1 : i - 1); break;
      case 'Home': go(0); break;
      case 'End': go(rows.length - 1); break;
      case 'PageDown': go(i + 12); break;
      case 'PageUp': go(i - 12); break;
      case 'ArrowRight':
        if (row?.kind === 'dir') { if (!row.open) toggle(row.entry.path, true); else go(i + 1); }
        break;
      case 'ArrowLeft':
        if (row?.kind === 'dir' && row.open && !q) toggle(row.entry.path, false);
        else if (row) { const up = parentOf(row.entry.path); if (up && index.has(up)) go(index.get(up)!); }
        break;
      case 'Enter': if (row) activate(row); break;
      case 'F2': actions.note(RENAME_WHY()); break;
      case 'Escape': if (filter) setFilter(''); else handled = false; break;
      default: handled = false;
    }
    if (handled) { e.preventDefault(); e.stopPropagation(); }
  };

  // the context menu: its own commands (labels, keys and reasons the way the menus show them)
  const menuCommands = useMemo<Commands>(() => {
    const row = menu?.row ?? null;
    const rel = row?.entry.path ?? '';
    const folder = folderOf(row);
    const c: Commands = {
      'ex.open': { label: row?.kind === 'image' || row?.kind === 'pdf' ? tr('가져와 놓기') : row?.kind === 'indesign' ? tr('프로젝트로 가져오기…') : row?.kind === 'dir' ? (row.open ? tr('접기') : tr('펼치기')) : tr('열기'), keys: ['Enter'],
        run: row && (!(row.kind === 'image' || row.kind === 'pdf') || actions.place) ? () => activate(row) : undefined,
        why: row && (row.kind === 'image' || row.kind === 'pdf') && !actions.place ? actions.placeWhy : undefined },
      'ex.newDocument': { label: tr('새 문서…'), run: actions.newDocument },
      'ex.newBook': { label: folder ? tr('이 폴더에 새 책…') : tr('새 책…'), run: () => actions.newBook(folder) },
      'ex.newLibrary': { label: folder ? tr('이 폴더에 새 라이브러리…') : tr('새 라이브러리…'), run: () => actions.newLibrary(folder) },
      'ex.newFolder': { label: folder ? tr('이 폴더에 새 폴더…') : tr('새 폴더…'), run: () => setFolderIn(folder) },
      'ex.import': { label: tr('InDesign 문서 가져오기…'), run: () => actions.importDoc() },
      'ex.reveal': { label: REVEAL_LABEL(), run: () => actions.reveal(rel ? actions.path(rel) : project!.info.root) },
      'ex.copyPath': { label: tr('프로젝트 상대 경로 복사'), run: rel ? () => { void navigator.clipboard.writeText(rel).then(() => actions.note(tr('복사했습니다: {0}', { 0: rel })), () => actions.note(tr('클립보드에 쓰지 못했습니다'))); } : undefined },
      'ex.rename': { label: tr('이름 바꾸기'), keys: ['F2'], why: RENAME_WHY() },
      'ex.delete': { label: tr('삭제'), why: DELETE_WHY() },
      'ex.refresh': { label: tr('새로 고침'), run: actions.refresh },
    };
    return c;
  }, [menu, actions, activate, project]);  // eslint-disable-line react-hooks/exhaustive-deps
  const menuItems: MenuEntry[] = menu?.row
    ? [{ cmd: 'ex.open' }, { sep: true }, { cmd: 'ex.newDocument' }, { cmd: 'ex.newBook' }, { cmd: 'ex.newLibrary' }, { cmd: 'ex.newFolder' }, { cmd: 'ex.import' }, { sep: true }, { cmd: 'ex.reveal' }, { cmd: 'ex.copyPath' }, { sep: true }, { cmd: 'ex.rename' }, { cmd: 'ex.delete' }]
    : [{ cmd: 'ex.newDocument' }, { cmd: 'ex.newBook' }, { cmd: 'ex.newLibrary' }, { cmd: 'ex.newFolder' }, { cmd: 'ex.import' }, { sep: true }, { cmd: 'ex.reveal' }, { cmd: 'ex.refresh' }];

  // the view's tab says "Explorer"; this head names the project and holds its buttons (the panel's fold is the dock's, uix-d2)
  const head = (
    <header className="sb-head ex-head">
      <span className="sb-twisty">
        <Icon name="project" size={14} />
        {project ? <b className="sb-title sb-project" title={project.info.root}>{project.info.name}</b> : <span className="sb-title">{tr('열린 프로젝트가 없습니다.')}</span>}
      </span>
      {project && (
        <span className="sb-actions">
          <button className="sb-act" onClick={actions.newDocument} title={tr('새 문서…')} aria-label={tr('새 문서…')}><Icon name="newPage" size={16} /></button>
          <button className="sb-act" onClick={() => actions.newBook(folderOf(sel !== null ? rows[index.get(sel)!] : null))} title={tr('새 책…')} aria-label={tr('새 책…')}><Icon name="bookPlus" size={16} /></button>
          <button className="sb-act" onClick={() => actions.newLibrary(folderOf(sel !== null ? rows[index.get(sel)!] : null))} title={tr('새 라이브러리…')} aria-label={tr('새 라이브러리…')}><Icon name="libraryPlus" size={16} /></button>
          <button className="sb-act" onClick={() => setFolderIn(folderOf(sel !== null ? rows[index.get(sel)!] : null))} title={tr('새 폴더…')} aria-label={tr('새 폴더…')}><Icon name="folderPlus" size={16} /></button>
          <button className="sb-act" onClick={actions.refresh} title={tr('새로 고침')} aria-label={tr('새로 고침')}><Icon name="refresh" size={16} /></button>
          <button className="sb-act" onClick={() => setOpen(Object.fromEntries(allDirs.map((d) => [d, false])))} title={tr('모두 접기')} aria-label={tr('모두 접기')}><Icon name="collapseAll" size={16} /></button>
        </span>
      )}
    </header>
  );
  if (!project || !tree) {
    return (
      <>
        <div className="ex-none">
          <p className="ex-none-lead">{tr('열린 프로젝트가 없습니다.')}</p>
          <p className="ex-none-text">{p.current ? tr('지금 문서는 프로젝트 없이 읽기 전용으로 열었습니다. 고치려면 프로젝트로 가져오세요.') : tr('Folio 는 폴더 하나를 프로젝트로 엽니다. 문서·책·라이브러리·그림이 모두 그 안에 있습니다.')}</p>
          <div className="ex-none-actions">
            <button className="primary" onClick={actions.newProject}><Icon name="plus" size={16} />{tr('새 프로젝트…')}</button>
            <button onClick={actions.openProject}><Icon name="folderOpen" size={16} />{tr('프로젝트 열기…')}</button>
          </div>
          <SampleCards compact />
          {p.recentProjects.length > 0 && (
            <>
              <h4 className="ex-group-title">{tr('최근 프로젝트')}</h4>
              <ul className="ex-recent">
                {p.recentProjects.slice(0, 8).map((r) => (
                  <li key={r.root}><button className="ex-row" onClick={() => actions.openRecent(r.root)} title={r.root}>
                    <Icon name="project" size={16} className="ex-kind" /><span className="ex-name">{r.name}</span>
                  </button></li>
                ))}
              </ul>
            </>
          )}
        </div>
      </>
    );
  }
  const current = p.current;
  const openRows: { key: string; title: string; dirty: boolean; active: boolean; pages: number | null; run?: () => void }[] = p.openDocs && p.openDocs.length > 1
    ? p.openDocs.map((d) => ({ key: `w${d.id}`, title: d.title ?? tr('시작 화면'), dirty: d.dirty, active: d.active, pages: d.active && current ? current.pages : null, run: d.active ? undefined : () => p.onActivateDoc(d.id) }))
    : current ? [{ key: 'this', title: current.name, dirty: current.dirty, active: true, pages: current.pages }] : [];
  return (
    <>
      {head}
      <label className="ex-filter">
        <Icon name="filter" size={14} />
        <input type="search" value={filter} aria-label={tr('이름으로 거르기')} placeholder={tr('이름으로 거르기')} spellCheck={false}
          onChange={(e) => setFilter(e.target.value)} onKeyDown={(e) => {
            if (e.key === 'Escape') { e.stopPropagation(); if (filter) setFilter(''); else treeRef.current?.focus(); }
            if (e.key === 'ArrowDown' || e.key === 'Enter') { e.preventDefault(); treeRef.current?.focus(); if (rows.length && sel === null) setSelected(rows[0].entry.path); }
          }} />
        {filter && <button className="ex-clear" onClick={() => setFilter('')} title={tr('거르기 지우기 (Esc)')} aria-label={tr('거르기 지우기 (Esc)')}><Icon name="close" size={12} /></button>}
      </label>
      {(tree.conflicts.length > 0 || tree.issues.length > 0 || tree.truncated) && (
        <div className="ex-alerts">
          {tree.conflicts.length > 0 && <p className="ex-alert bad"><Icon name="warning" size={14} />{tr('동기화 충돌 사본 {0}개', { 0: tree.conflicts.length })}</p>}
          {tree.issues.length > 0 && <p className="ex-alert warn" title={tree.issues.map((i) => `${i.folder}/${i.name}: ${i.kind} ${i.detail ?? ''}`).join('\n')}><Icon name="warning" size={14} />{tr('이름 문제 {0}개', { 0: tree.issues.length })}</p>}
          {tree.truncated && <p className="ex-alert warn"><Icon name="warning" size={14} />{tr('항목이 너무 많아 앞의 20,000개만 보입니다')}</p>}
        </div>
      )}
      <div className="ex-scroll" onContextMenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, row: null }); } }}>
        {openRows.length > 0 && !q && (
          <div className="ex-group">
            <button className="ex-group-head" aria-expanded={openGroup} onClick={() => setOpenGroup((v) => !v)}>
              <Icon name={openGroup ? 'chevronDown' : 'chevronRight'} size={12} />{tr('열린 문서')}<span className="ex-count">{openRows.length}</span>
            </button>
            {openGroup && openRows.map((d) => (
              <button key={d.key} className={`ex-row open-doc${d.active ? ' current' : ''}`} onClick={d.run} title={d.active ? tr('지금 이 창의 문서') : tr('이 문서의 창으로 갑니다')}>
                <Icon name="document" size={16} className="ex-kind" />
                <span className="ex-name">{d.title}</span>
                {d.pages !== null && <span className="ex-meta">{tr('{0}쪽', { 0: d.pages })}</span>}
                {d.dirty && <span className="ex-dirty" title={tr('저장하지 않은 변경')} />}
              </button>
            ))}
          </div>
        )}
        <div className="ex-tree" ref={treeRef} role="tree" aria-label={tr('프로젝트 파일')} tabIndex={0}
          aria-activedescendant={sel !== null ? `ex-r${index.get(sel)}` : undefined}
          onKeyDown={onKey} onFocus={() => { if (rows.length) setSelected((cur) => (cur !== null && index.has(cur) ? cur : rows[0].entry.path)); }}
          onContextMenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, row: null }); } }}>
          {rows.length === 0 && <p className="p-empty ex-empty">{q ? tr('"{0}"이(가) 들어간 이름이 없습니다.', { 0: filter.trim() }) : tr('프로젝트 폴더가 비어 있습니다. 새 문서를 만들거나 InDesign 문서를 가져오세요.')}</p>}
          {rows.map((r, i) => <TreeRow key={r.entry.path} id={`ex-r${i}`} row={r} doc={docs.get(r.entry.path)} current={current?.rel === r.entry.path} currentPages={current?.pages ?? null}
            selected={sel === r.entry.path} q={q}
            onSelect={() => { setSelected(r.entry.path); treeRef.current?.focus({ preventScroll: true }); }}
            onToggle={() => toggle(r.entry.path)} onActivate={() => activate(r)}
            onMenu={(x, y) => { setSelected(r.entry.path); setMenu({ x, y, row: r }); }}
            dragFile={(r.kind === 'image' || r.kind === 'pdf') ? actions.path(r.entry.path) : null} />)}
        </div>
      </div>
      <footer className="ex-foot">{tr('문서 {0} · 파일 {1}', { 0: counts.documents, 1: counts.files })}</footer>
      {menu && <PopupMenu items={menuItems} commands={menuCommands} anchor={new DOMRect(menu.x, menu.y - 2, 0, 0)} onClose={() => setMenu(null)} />}
      {folderIn !== null && (
        <NameDialog title={folderIn ? tr('새 폴더 — {0}', { 0: folderIn }) : tr('새 폴더')} label={tr('이름:')} initial={tr('새 폴더')}
          taken={(n) => siblings(tree.entries, folderIn).has(fold(n))} close={() => setFolderIn(null)}
          onOk={(name) => {
            const where = folderIn;
            void actions.newFolder(where, name).then((made) => {
              // the folders on the way are opened and the new one is selected
              setOpen((o) => { const next = { ...o }; let up = where; while (up) { next[up] = true; up = parentOf(up); } return next; });
              setSelected(made);
              reveal(made);
            }, (e) => actions.note(String((e as Error).message ?? e)));
          }} />
      )}
    </>
  );
}

/** The names already in a folder of the tree (folded the way the engine compares them), for New Folder's "taken". */
function siblings(entries: TreeEntry[], folder: string): Set<string> {
  let list = entries;
  for (const part of folder ? folder.split('/') : []) list = list.find((e) => e.kind === 'dir' && e.name === part)?.children ?? [];
  return new Set(list.map((e) => fold(e.name)));
}

/** A file or folder of the tree: its kind's icon, its name (the part that matches the filter marked), and what is wrong with it. */
function TreeRow({ id, row, doc, current, currentPages, selected, q, onSelect, onToggle, onActivate, onMenu, dragFile }: {
  id: string; row: Row; doc?: ProjectTree['documents'][number]; current: boolean; currentPages: number | null; selected: boolean; q: string;
  onSelect: () => void; onToggle: () => void; onActivate: () => void; onMenu: (x: number, y: number) => void; dragFile: string | null;
}) {
  const e = row.entry;
  const missing = doc ? doc.links - doc.linksPresent : 0;
  const lockedByOther = !!doc?.lockedBy && !doc.lockedBy.stale && !doc.lockedBy.mine;
  const name = useMemo(() => {
    if (!q) return e.name;
    const at = fold(e.name).indexOf(q);
    if (at < 0) return e.name;
    // NFC folding keeps the length of Hangul and Latin names, so the match marks the same letters in the shown name
    return <>{e.name.slice(0, at)}<mark>{e.name.slice(at, at + q.length)}</mark>{e.name.slice(at + q.length)}</>;
  }, [e.name, q]);
  const tip = e.conflict ? tr('동기화 충돌 사본 ({0}) — 다른 컴퓨터에서 동시에 고친 흔적입니다', { 0: e.conflict }) : `${e.path}\n${KIND_NAME[row.kind]()}`;
  return (
    <div id={id} role="treeitem" aria-level={row.depth + 1} aria-selected={selected} aria-expanded={row.kind === 'dir' ? row.open : undefined}
      data-path={e.path} data-kind={row.kind}
      className={`ex-row ${row.kind}${selected ? ' sel' : ''}${current ? ' on current' : ''}${e.conflict ? ' conflict' : ''}`}
      title={tip}
      draggable={!!dragFile}
      onDragStart={dragFile ? (ev) => { ev.dataTransfer.setData('application/x-folio-place', dragFile); ev.dataTransfer.effectAllowed = 'copy'; } : undefined}
      onPointerDown={(ev) => { if (ev.button === 0) onSelect(); }}
      onClick={() => { if (row.kind === 'dir') onToggle(); }}
      onDoubleClick={() => { if (row.kind !== 'dir') onActivate(); }}
      onContextMenu={(ev) => { ev.preventDefault(); ev.stopPropagation(); onMenu(ev.clientX, ev.clientY); }}>
      {row.depth > 0 && <span className="ex-indent" style={{ width: row.depth * 12 }} />}
      <span className="ex-twisty">{row.kind === 'dir' && row.hasKids ? <Icon name={row.open ? 'chevronDown' : 'chevronRight'} size={12} /> : null}</span>
      <Icon name={row.kind === 'dir' && row.open ? 'folderOpen' : ICON[row.kind]} size={16} className="ex-kind" />
      <span className="ex-name">{name}</span>
      {e.conflict && <span className="ex-badge bad">{tr('충돌')}</span>}
      {lockedByOther && <span className="ex-badge warn" title={`${doc!.lockedBy!.user} @ ${doc!.lockedBy!.machine}`}>{tr('사용 중')}</span>}
      {doc?.error && <span className="ex-badge bad" title={doc.error}>{tr('읽지 못함')}</span>}
      {missing > 0 && <span className="ex-badge warn" title={tr('없는 링크 {0}개 (가져올 때 없던 링크 {1}개 포함)', { 0: missing, 1: doc!.linksMissing })}><Icon name="links" size={12} />{missing}</span>}
      {current && currentPages !== null && <span className="ex-meta">{tr('{0}쪽', { 0: currentPages })}</span>}
    </div>
  );
}

const RENAME_WHY = () => tr('이름 바꾸기는 책과 문서의 참조를 함께 고치는 엔진 명령이 아직 없어 여기서는 할 수 없습니다.');
const DELETE_WHY = () => tr('지우기는 이 파일을 쓰는 문서·책을 확인하는 엔진 명령이 생기면 됩니다.');
const REVEAL_LABEL = () => (/mac/i.test(navigator.platform) ? tr('Finder 에서 보기') : tr('파일 탐색기에서 보기'));

void Fragment;
