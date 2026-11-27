import { SampleCards } from './ui/SampleCards';
import { t as tr } from './i18n';
import { hasTranslation, useLanguage } from './i18n';
import { engineMessage, type EngineMessage } from './i18n/engine-message';
// Folio projects in the window: the start screen (new / open / recent projects, import), the
// import dialog (target project, document path, plan, progress), the project explorer and
// the read-only banner. The engine owns every rule (folio-project crate); this file only asks
// and shows.
import { useCallback, useEffect, useMemo, useState } from 'react';
import './project.css';
import { LimitsBlock, type LimitsReport } from './ui/limits';
import { Icon } from './ui/icons';

// ---------------------------------------------------------------------------------------
// engine calls and types

export type ProjectInfo = { root: string; id: string; name: string; created?: string };

export type TreeEntry = { path: string; name: string; kind: 'dir' | 'document' | 'file'; bytes: number; children: TreeEntry[]; conflict?: string };

export type DocumentInfo = {
  path: string;
  id: string | null;
  links: number;
  linksPresent: number;
  linksMissing: number;
  files: number;
  filesPresent: number;
  lockedBy: { user: string; machine: string; time: string; stale: boolean; mine: boolean } | null;
  error: string | null;
};

export type ProjectTree = {
  root: string;
  id: string;
  name: string;
  entries: TreeEntry[];
  documents: DocumentInfo[];
  conflicts: { path: string; kind: string; original: string | null }[];
  issues: { folder: string; name: string; kind: string; other: string | null; detail: string | null }[];
  truncated: boolean;
};

export type LockState =
  | { readOnly: false; replacedStale?: { user: string; machine: string; time: string } | null }
  | { readOnly: true; user: string; machine: string; time: string; ageSecs: number | null; sameMachine: boolean; pid: number };

/** The `folio` part of an opened document's summary. */
export type FolioSummary = {
  project: ProjectInfo;
  document: { id: string; path: string; file: string; source: { name: string; kind: string; bytes: number; sha256: string } | null; created: string; saved: string };
  references: { references: number; ok: number; changed: number; moved: number; missing: number; notInProject: number; linksInProject: number; linksResolved: number; hashedFiles: number; indexHits: number; resolveMs: number; pinsChanged: number; longPaths: number };
  pinChanges: { reference: string; path: string }[];
  warnings: string[];
  lock: LockState;
  readOnly: boolean;
};

export type ImportPlan = {
  source: { name: string; kind: string; bytes: number; pages: number; stories: number };
  document: string;
  documentExists: boolean;
  suggestion: string | null;
  assets: string;
  links: {
    placements: number; files: number; inside: { count: number; bytes: number }; outside: { count: number; bytes: number };
    missing: number; ambiguous: number; embedded: number; modified: number; modifiedNames: string[]; missingNames: string[]; ambiguousRows: { name: string; candidates: number }[];
  };
  previews: { count: number; bytes: number };
  embedded: { files: number; bytes: number; graphics: number };
  fonts: { family: string; style: string; action: string; code: string | null; reason: string | null; file: string | null; version: string | null }[];
  fontCounts: { used: number; copy: number; record: number; missing: number; substituted: number };
  otherFiles: { name: string; found: boolean }[];
  renamed: { from: string; to: string }[];
  longPaths: string[];
  copy: { count: number; bytes: number };
  warnings: string[];
  warningMessages?: EngineMessage[];
  /** Differences from InDesign this document uses (principle 2, item 3) and the fixed ones of this version; see ui/limits.tsx. */
  limits?: LimitsReport;
  timings?: Record<string, number>;
};

export type ImportResult = { document: string; path: string; id: string; folioBytes: number; references: number; filesWritten: number; bytesWritten: number; deduplicated: number; timings: Record<string, unknown> };

export class EngineError extends Error {
  constructor(message: string, readonly code: string | null) {
    super(message);
  }
}

async function call<T>(message: Record<string, unknown>, key: string): Promise<T> {
  const { header } = await window.folio.request(message);
  if (!header.ok) throw new EngineError(header.error ?? `${message.cmd} failed`, header.code ?? null);
  return header[key] as T;
}

export const projectInfo = async (path: string) => {
  const { header } = await window.folio.request({ cmd: 'projectInfo', path });
  if (!header.ok) throw new EngineError(header.error ?? 'projectInfo failed', header.code ?? null);
  return { project: header.project as ProjectInfo, tree: header.tree as ProjectTree };
};
export const createProject = (dir: string, name: string) => call<ProjectInfo>({ cmd: 'projectCreate', dir, name }, 'project');
export type ImportRequest = { source: string; project: string; document: string; roots: string[] };
export const importPlan = (r: ImportRequest) => call<ImportPlan>({ cmd: 'importPlan', ...r }, 'plan');
export const importRun = (r: ImportRequest) => call<ImportResult>({ cmd: 'importRun', ...r }, 'result');
/** The installed families with their styles, and the font a new document starts with. */
export type NewDocumentFonts = { fonts: { family: string; styles: { style: string; postscript: string }[] }[]; default: { family: string; style: string } | null };
export const newDocumentFonts = () => window.folio.request({ cmd: 'newDocumentFonts' }).then(({ header }) => {
  if (!header.ok) throw new EngineError(header.error ?? 'newDocumentFonts failed', header.code ?? null);
  return { fonts: header.fonts, default: header.default } as NewDocumentFonts;
});
/** File ▸ New ▸ Document…: the engine holds the dialog (next/shared/dialogs/newDocument.json, an app dialog) for the open project;
 *  it starts from the document preset `docpreset` ('' is [Default]). The names the new document gets ("제목 없음-1", its first layer)
 *  are in the screen's language; applying makes and imports the document (the result's `result.path`, which the screen opens). */
export const openNewDocument = (project: string, docpreset = '') => window.folio.request({ cmd: 'openDialog', dialog: 'newDocument',
  values: { project, docpreset, untitled: tr('제목 없음-{0}'), layer: tr('레이어 1') } });
/** File ▸ Document Presets ▸ Define… ▸ New / Edit: the preset editor (documentPreset.json); `replacing` is the preset edited, '' a new one. */
export const openDocumentPreset = (replacing = '') => window.folio.request({ cmd: 'openDialog', dialog: 'documentPreset', values: { replacing } });
/** Removes a document preset from the user's settings (the menu follows the settings event). */
export const deleteDocumentPreset = (name: string) => window.folio.request({ cmd: 'documentPresets', action: 'delete', name });
export const importCancel = () => window.folio.request({ cmd: 'importCancel' });
export const saveDocument = () => window.folio.request({ cmd: 'save' }).then(({ header }) => {
  if (!header.ok) throw new EngineError(header.error ?? 'save failed', header.code ?? null);
  return header as unknown as { path: string; bytes: number; ms: number; healed: number };
});
/** File ▸ Save a Copy: the document as it is now to another .folio in the project (a new id). */
export const saveDocumentCopy = (path: string) => window.folio.request({ cmd: 'saveCopy', path }).then(({ header }) => {
  if (!header.ok) throw new EngineError(header.error ?? 'save copy failed', header.code ?? null);
  return header as unknown as { path: string; bytes: number; ms: number };
});
export const takeOverLock = () => call<LockState>({ cmd: 'lockOverride' }, 'lock');
type IdmlExportReport = {
  bytes: number;
  differences: { what: string; count: number; effect: string }[];
  limits?: LimitsReport;
};
export const exportIdml = (path: string) => call<IdmlExportReport>({ cmd: 'exportIdml', path }, 'report');

// bridge methods of the project UI (preload.cjs `folioProject`)
export type RecentProject = { root: string; id: string; name: string };
declare global {
  interface Window {
    folioProject: {
      recent(): Promise<RecentProject[]>;
      remember(p: RecentProject): Promise<RecentProject[]>;
      /** A file dialog for .folio / .indd / .indt / .idml. */
      openAny(): Promise<string | null>;
      saveIdml(defaultName: string): Promise<string | null>;
      /** A save dialog for a .folio (File ▸ Save As / Save a Copy); `defaultPath` beside the document. */
      saveFolio(defaultPath: string): Promise<string | null>;
      setTitle(title: string): Promise<void>;
    };
  }
}

export const isImportable = (p: string) => /\.(indd|indt|idml)$/i.test(p);
export const isFolio = (p: string) => /\.folio$/i.test(p);
export const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
export const size = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(2)} GB` : b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`);
const joinPath = (root: string, rel: string) => {
  const sep = /^[A-Za-z]:[\\/]|^\\\\/.test(root) ? '\\' : '/';
  return `${root.replace(/[\\/]+$/, '')}${sep}${rel.replace(/[\\/]/g, sep)}`;
};
export const documentPath = (root: string, rel: string) => joinPath(root, rel);

function age(secs: number | null) {
  if (secs === null) return '';
  if (secs < 90) return tr('{0}초 전', {0: Math.max(1, Math.round(secs))});
  if (secs < 5400) return tr('{0}분 전', {0: Math.round(secs / 60)});
  if (secs < 129600) return tr('{0}시간 전', {0: Math.round(secs / 3600)});
  return tr('{0}일 전', {0: Math.round(secs / 86400)});
}

// ---------------------------------------------------------------------------------------
// start screen

export function StartScreen({
  recentProjects,
  recentDocuments,
  onNewProject,
  onOpenProject,
  onImport,
  onOpenRecentProject,
}: {
  recentProjects: RecentProject[];
  recentDocuments: string[];
  onNewProject: () => void;
  onOpenProject: () => void;
  onImport: (path?: string) => void;
  onOpenRecentProject: (p: RecentProject) => void;
}) {
  // The start sheet (uix-b3): the brand line, the actions in a column (the one primary first), the recent lists beside them, and on
  // the first run (no recent project) the three steps of a project folder in place of an empty list. Trim marks frame the sheet (CSS).
  const firstRun = recentProjects.length === 0;
  return (
    <div className="start" data-first-run={firstRun || undefined}>
      <header className="start-head">
        <span className="start-mark" aria-hidden="true">Fo</span>
        <div>
          <h1>Folio Studio</h1>
          <p className="start-lede">{tr('잡지 한 호를 프로젝트 폴더 하나로 만듭니다. InDesign 문서를 잃지 않고 가져와 편집하고 인쇄소에 넘깁니다.')}</p>
        </div>
      </header>
      <div className="start-body">
        <nav className="start-actions" aria-label={tr('시작')}>
          <button className="primary" onClick={onNewProject}><Icon name="plus" size={16} />{tr('새 프로젝트…')}</button>
          <button onClick={onOpenProject}><Icon name="folder" size={16} />{tr('프로젝트 열기…')}</button>
          <button onClick={() => onImport()}><Icon name="file" size={16} />{tr('InDesign 문서 가져오기…')}</button>
          <SampleCards />
        </nav>
        <div className="start-lists">
          <section aria-labelledby="start-recent-projects">
            <h2 id="start-recent-projects">{tr('최근 프로젝트')}{!firstRun && <span className="start-count">{recentProjects.length}</span>}</h2>
            {firstRun && (
              <div className="start-empty">
                <p>{tr('최근 프로젝트가 여기에 나옵니다. Folio는 폴더 하나를 프로젝트로 엽니다:')}</p>
                <ol className="start-steps">
                  <li><b>{tr('프로젝트를 만듭니다')}</b><span>{tr('폴더에 folio.project 파일이 생깁니다. 폴더를 NAS나 다른 컴퓨터로 통째로 옮겨도 그대로 열립니다.')}</span></li>
                  <li><b>{tr('InDesign 문서를 가져옵니다')}</b><span>{tr('링크한 그림을 프로젝트 안으로 복사합니다. 원본 INDD·IDML은 읽기만 합니다.')}</span></li>
                  <li><b>{tr('편집하고 내보냅니다')}</b><span>{tr('PDF/X로 인쇄소에, IDML로 InDesign에 넘깁니다. 패키지도 됩니다.')}</span></li>
                </ol>
              </div>
            )}
            {recentProjects.map((p) => (
              <button key={p.id || p.root} className="start-item" onClick={() => onOpenRecentProject(p)} title={p.root}>
                <Icon name="project" size={16} />
                <span className="start-item-text"><b>{p.name}</b><span>{p.root}</span></span>
              </button>
            ))}
          </section>
          {recentDocuments.length > 0 && (
            <section aria-labelledby="start-recent-documents">
              <h2 id="start-recent-documents">{tr('최근 InDesign 문서 ')}<span className="muted">{tr('(프로젝트로 가져오기)')}</span></h2>
              {recentDocuments.map((d) => (
                <button key={d} className="start-item" onClick={() => onImport(d)} title={d}>
                  <Icon name="document" size={16} />
                  <span className="start-item-text"><b>{baseName(d)}</b><span>{d}</span></span>
                </button>
              ))}
            </section>
          )}
        </div>
      </div>
      <p className="start-hint"><Icon name="file" size={16} /><span>{tr('INDD·IDML 파일을 창에 끌어다 놓으면 열기 창이 뜹니다: 프로젝트로 가져오기, 또는 프로젝트 없이 읽기 전용으로 열기(PDF·패키지는 됨). 원본은 읽기만 하고 덮어쓰지 않습니다.')}</span></p>
    </div>
  );
}

/** A project open, no document: the project's name and place, and what to do next (new document, import, the sample). */
export function ProjectHome({ project, onNewDocument, onImport }: { project: ProjectInfo; onNewDocument?: () => void; onImport: () => void }) {
  return (
    <div className="project-home">
      <header className="start-head">
        <span className="start-mark project" aria-hidden="true"><Icon name="project" size={20} /></span>
        <div>
          <h1>{project.name}</h1>
          <p className="project-home-path" title={project.root}>{project.root}</p>
        </div>
      </header>
      <div className="start-actions project-home-actions">
        {onNewDocument && <button className="primary" onClick={onNewDocument}><Icon name="newPage" size={16} />{tr('새 문서…')}</button>}
        <button onClick={onImport}><Icon name="file" size={16} />{tr('InDesign 문서 가져오기…')}</button>
      </div>
      <SampleCards />
      <p className="start-hint"><Icon name="project" size={16} /><span>{tr('오른쪽 프로젝트 패널에서 문서를 두 번 눌러 엽니다. InDesign 문서는 ＋ 로 가져옵니다.')}</span></p>
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// new project

/** File ▸ New ▸ Project…: the engine holds the dialog (next/shared/dialogs/newProject.json, an app dialog: no document needed); the app
 *  frame draws it and opens the project it made. The name starts in the user's language. */
export const openNewProject = () => void window.folio.request({ cmd: 'openDialog', dialog: 'newProject', values: { name: tr('새 프로젝트') } });

// ---------------------------------------------------------------------------------------
// import dialog

type Target = { kind: 'current' | 'existing' | 'new'; root: string; name: string };
type Phase =
  | { kind: 'choose' }
  | { kind: 'planning'; stage: string }
  | { kind: 'planned'; plan: ImportPlan }
  | { kind: 'running'; stage: string; done: number; total: number; files: number; filesTotal: number; cancelling: boolean }
  | { kind: 'error'; message: string; code: string | null };

const PLAN_STAGES: Record<string, string> = { get reader() { return tr('INDD 읽기'); }, get parse() { return tr('문서 해석'); }, get links() { return tr('링크 찾기'); }, get plan() { return tr('계획 세우기'); } };
const RUN_STAGES: Record<string, string> = { get copy() { return tr('파일 복사·해시'); }, get place() { return tr('프로젝트에 넣기'); }, get write() { return tr('.folio 쓰기'); }, get done() { return tr('끝'); } };

export function ImportDialog({
  source,
  current,
  roots,
  onDone,
  onClose,
  onReadOnly,
}: {
  source: string;
  current: ProjectInfo | null;
  roots: string[];
  onDone: (folio: string, project: ProjectInfo) => void;
  onClose: () => void;
  /** Opens the file as it is, without a project: nothing saved, no editing, output allowed. */
  onReadOnly?: () => void;
}) {
  const stem = baseName(source).replace(/\.(indd|indt|idml)$/i, '');
  const [mode, setMode] = useState<'import' | 'readOnly'>('import');
  const [target, setTarget] = useState<Target>(current ? { kind: 'current', root: current.root, name: current.name } : { kind: 'new', root: '', name: stem });
  const [document, setDocument] = useState(`${stem}.folio`);
  const [phase, setPhase] = useState<Phase>({ kind: 'choose' });
  const [newParent, setNewParent] = useState('');

  useEffect(
    () =>
      window.folio.onEvent((e) => {
        if (e.job !== 'import') return;
        setPhase((p) => {
          if (p.kind === 'planning' && e.stage) return { kind: 'planning', stage: e.stage };
          if (p.kind === 'running') return { ...p, stage: e.stage ?? p.stage, done: e.done ?? p.done, total: e.total ?? p.total, files: e.files ?? p.files, filesTotal: e.filesTotal ?? p.filesTotal };
          return p;
        });
      }),
    [],
  );

  const pickExisting = async () => {
    const dir = await window.folio.chooseFolder(tr('Folio 프로젝트 폴더(folio.project가 있는 폴더)'));
    if (!dir) return;
    try {
      const { project } = await projectInfo(dir);
      setTarget({ kind: 'existing', root: project.root, name: project.name });
      setPhase({ kind: 'choose' });
    } catch (e) {
      setPhase({ kind: 'error', message: `${dir}: ${(e as Error).message}`, code: 'notProject' });
    }
  };
  const pickNewParent = async () => {
    const dir = await window.folio.chooseFolder(tr('새 프로젝트를 만들 위치'));
    if (dir) {
      setNewParent(dir);
      setTarget({ kind: 'new', root: joinPath(dir, target.name || stem), name: target.name || stem });
    }
  };

  const ensureProject = async (): Promise<ProjectInfo> => {
    if (target.kind !== 'new') return { root: target.root, id: '', name: target.name };
    if (!newParent) throw new EngineError(tr('새 프로젝트 폴더의 위치를 고르세요'), 'noParent');
    return createProject(joinPath(newParent, target.name.trim() || stem), target.name.trim() || stem);
  };

  const request = (root: string): ImportRequest => ({ source, project: root, document: document.trim(), roots });

  const plan = async () => {
    setPhase({ kind: 'planning', stage: 'reader' });
    try {
      const project = await ensureProject();
      if (target.kind === 'new') setTarget({ kind: 'existing', root: project.root, name: project.name });
      const p = await importPlan(request(project.root));
      setPhase({ kind: 'planned', plan: p });
    } catch (e) {
      setPhase({ kind: 'error', message: (e as Error).message, code: (e as EngineError).code ?? null });
    }
  };

  const run = async () => {
    if (phase.kind !== 'planned') return;
    const total = phase.plan.copy.bytes;
    setPhase({ kind: 'running', stage: 'copy', done: 0, total, files: 0, filesTotal: phase.plan.copy.count, cancelling: false });
    try {
      const r = await importRun(request(target.root));
      onDone(r.path, { root: target.root, id: '', name: target.name });
    } catch (e) {
      setPhase({ kind: 'error', message: (e as Error).message, code: (e as EngineError).code ?? null });
    }
  };

  const cancel = () => {
    if (phase.kind === 'running' || phase.kind === 'planning') {
      importCancel();
      if (phase.kind === 'running') setPhase({ ...phase, cancelling: true });
    } else onClose();
  };

  const busy = phase.kind === 'planning' || phase.kind === 'running';
  const p = phase.kind === 'planned' ? phase.plan : null;
  const targetReady = target.kind === 'new' ? !!newParent && !!target.name.trim() : !!target.root;
  return (
    <div className="overlay">
      <div className="card dialog import">
        <h2>{tr('InDesign 문서 열기 ')}<span>{baseName(source)}</span></h2>
        {onReadOnly && (
          <section>
            <h3>{tr('열기 방식')}</h3>
            <label>
              <input type="radio" disabled={busy} checked={mode === 'import'} onChange={() => setMode('import')} />
              <b>{tr('프로젝트로 가져오기')}</b> <span className="muted small">{tr('— 편집·저장 (.folio). 링크를 프로젝트 안으로 복사합니다')}</span>
            </label>
            <label>
              <input type="radio" disabled={busy} checked={mode === 'readOnly'} onChange={() => setMode('readOnly')} />
              <b>{tr('읽기 전용으로 열기')}</b> <span className="muted small">{tr('— 프로젝트 없이 바로 봅니다')}</span>
            </label>
            {mode === 'readOnly' && (
              <p className="muted small read-only-note">{tr('아무것도 저장하지 않고 프로젝트 폴더도 만들지 않습니다. 편집·저장은 할 수 없고, 사전 검사·링크 확인·분판 미리보기·PDF 내보내기·패키지는 됩니다. 고치고 싶어지면 나중에 같은 문서를 프로젝트로 가져오세요.')}</p>
            )}
          </section>
        )}
        {mode === 'import' && <>
        <section>
          <h3>{tr('넣을 프로젝트')}</h3>
          {current && (
            <label>
              <input type="radio" disabled={busy} checked={target.kind === 'current'} onChange={() => { setTarget({ kind: 'current', root: current.root, name: current.name }); setPhase({ kind: 'choose' }); }} />
              {tr('지금 프로젝트 ')}<b>{current.name}</b>
            </label>
          )}
          <label>
            <input type="radio" disabled={busy} checked={target.kind === 'existing'} onChange={pickExisting} />
            {tr('기존 프로젝트')}{target.kind === 'existing' ? <b>{target.name}</b> : '…'}
            {target.kind === 'existing' && <button className="link-button" disabled={busy} onClick={pickExisting}>{tr('바꾸기')}</button>}
          </label>
          <label>
            <input type="radio" disabled={busy} checked={target.kind === 'new'} onChange={() => { setTarget({ kind: 'new', root: newParent ? joinPath(newParent, stem) : '', name: stem }); setPhase({ kind: 'choose' }); }} />
            {tr('새 프로젝트 폴더')}</label>
          {target.kind === 'new' && (
            <div className="import-new">
              <div className="pkg-dest">
                <span className="path" title={newParent}>{newParent || tr('위치를 고르세요')}</span>
                <button disabled={busy} onClick={pickNewParent}>{tr('위치…')}</button>
              </div>
              <label className="pkg-name">
                {tr('이름')}<input value={target.name} disabled={busy} onChange={(e) => setTarget({ ...target, name: e.target.value, root: newParent ? joinPath(newParent, e.target.value) : '' })} />
              </label>
            </div>
          )}
          {target.root && <p className="muted small path-line" title={target.root}>{target.root}</p>}
        </section>
        <section>
          <h3>{tr('문서 (프로젝트 안 경로)')}</h3>
          <input value={document} disabled={busy} onChange={(e) => { setDocument(e.target.value); setPhase({ kind: 'choose' }); }} />
          <p className="muted small">{tr('언제나 .folio로 저장합니다. 원본 {0}은(는) 읽기만 합니다.', {0: baseName(source)})}</p>
        </section>
        </>}

        {phase.kind === 'planning' && (
          <section className="pkg-progress">
            <p className="stage"><span>{PLAN_STAGES[phase.stage] ?? phase.stage}…</span></p>
            <div className="bar"><div style={{ width: '35%' }} /></div>
          </section>
        )}
        {p && <PlanView plan={p} onUseSuggestion={(s) => { setDocument(s); setPhase({ kind: 'choose' }); }} />}
        {phase.kind === 'running' && (
          <section className="pkg-progress">
            <p className="stage">
              <span>{phase.cancelling ? tr('취소하는 중…') : RUN_STAGES[phase.stage] ?? phase.stage}</span>
              <span>{tr('{0} / {1} · 파일 {2}/{3}', {0: size(phase.done), 1: size(phase.total), 2: phase.files, 3: phase.filesTotal})}</span>
            </p>
            <div className="bar"><div style={{ width: `${phase.total ? Math.min(100, (100 * phase.done) / phase.total) : 5}%` }} /></div>
            <p className="muted small">{tr('복사는 프로젝트의 임시 폴더에서 하고 마지막에 넣습니다. 취소하거나 실패하면 프로젝트에 아무것도 남지 않습니다.')}</p>
          </section>
        )}
        {phase.kind === 'error' && (phase.code === 'cancelled'
          ? <p className="dialog-alert warn" role="status"><Icon name="warning" size={16} /><span>{tr('가져오기를 취소했습니다. 프로젝트에는 아무것도 남지 않았습니다.')}</span></p>
          : <p className="dialog-alert" role="alert"><Icon name="warning" size={16} /><span>{phase.message}</span></p>)}

        <div className="actions">
          <button onClick={cancel}>{busy ? tr('취소') : tr('닫기')}</button>
          {mode === 'readOnly' && onReadOnly && <button className="primary" onClick={onReadOnly}>{tr('읽기 전용으로 열기')}</button>}
          {mode === 'import' && !p && <button className="primary" disabled={busy || !targetReady || !document.trim()} onClick={plan}>{tr('계획 보기')}</button>}
          {mode === 'import' && p && <button className="primary" disabled={p.documentExists} onClick={run}>{tr('가져오기')}</button>}
        </div>
      </div>
    </div>
  );
}

function PlanView({ plan: p, onUseSuggestion }: { plan: ImportPlan; onUseSuggestion: (s: string) => void }) {
  const l = p.links;
  const missingFonts = p.fonts.filter((f) => f.action === 'missing' || f.action === 'substituted');
  return (
    <section className="import-plan">
      <h3>{tr('가져올 내용 ')}<span className="muted">{tr('— {0}쪽, 기사 {1}개, {2}', {0: p.source.pages, 1: p.source.stories, 2: size(p.source.bytes)})}</span></h3>
      <div className="plan-grid">
        <span>{tr('링크')}</span>
        <span>
          {tr('배치 ')}{l.placements}{tr('개, 파일 ')}{l.files}{tr('개 — 프로젝트 안 ')}<b>{l.inside.count}</b>({size(l.inside.bytes)}{tr(', 그대로), 밖 ')}<b>{l.outside.count}</b>({size(l.outside.bytes)}{tr(', 복사)')}{l.missing > 0 && <>, <b className="warn">{tr('없음 {0}', {0: l.missing})}</b>{tr('(미리보기 보존)')}</>}
          {l.ambiguous > 0 && <>, <b className="warn">{tr('후보 여럿 {0}', {0: l.ambiguous})}</b>{tr('(고르지 않음)')}</>}
          {l.embedded > 0 && <>{tr(', 문서에 포함 {0}', {0: l.embedded})}</>}
          {l.modified > 0 && <>, <b className="warn">{tr('크기가 다른 파일 {0}', {0: l.modified})}</b>{tr('(배치한 것과 다름: 수정됨으로 표시)')}</>}
        </span>
        <span>{tr('미리보기')}</span>
        <span>{tr('{0}개, {1} → {2}/previews', {0: p.previews.count, 1: size(p.previews.bytes), 2: p.assets})}</span>
        <span>{tr('포함·붙여 넣은 그림')}</span>
        <span>{p.embedded.files ? <>{tr('원본 데이터 {0}개, {1} → {2}/embedded', {0: p.embedded.files, 1: size(p.embedded.bytes), 2: p.assets})}</> : tr('없음')}</span>
        <span>{tr('글꼴')}</span>
        <span>
          {p.fontCounts.used}{tr('개 중 복사 ')}{p.fontCounts.copy}{tr(', 이름·버전·해시만 기록 ')}{p.fontCounts.record}
          {missingFonts.length > 0 && <>, <b className="warn">{tr('없음·대체 {0}', {0: missingFonts.length})}</b></>}
        </span>
        <span>{tr('이름 고침')}</span>
        <span>{p.renamed.length ? tr('{0}개 (모든 OS에서 쓸 수 있는 이름으로; 원래 이름은 기록)', {0: p.renamed.length}) : tr('없음')}</span>
        <span>{tr('복사')}</span>
        <span><b>{size(p.copy.bytes)}</b> ({p.copy.count}{tr('개 파일; 내용이 같은 파일은 한 번만)')}</span>
      </div>
      {(l.missingNames.length > 0 || l.ambiguousRows.length > 0) && (
        <details>
          <summary className="small">{tr('없는 링크 {0}개 보기', {0: l.missing + l.ambiguous})}</summary>
          <ul className="pkg-lines">
            {l.missingNames.map((n) => <li key={n}>{n}</li>)}
            {l.ambiguousRows.map((r) => <li key={r.name} className="warn">{tr('{0} — 후보 {1}개', {0: r.name, 1: r.candidates})}</li>)}
          </ul>
        </details>
      )}
      {l.modified > 0 && (
        <details>
          <summary className="small">{tr('크기가 다른 링크 {0}개 보기', {0: l.modified})}</summary>
          <ul className="pkg-lines">{l.modifiedNames.map((n) => <li key={n} className="warn">{n}</li>)}</ul>
        </details>
      )}
      {p.renamed.length > 0 && (
        <details>
          <summary className="small">{tr('바뀌는 이름')}</summary>
          <ul className="pkg-lines">{p.renamed.map((r) => <li key={r.from}>{r.from} → {r.to}</li>)}</ul>
        </details>
      )}
      <LimitsBlock limits={p.limits} />
      {p.longPaths.length > 0 && <p className="warnline small">{tr('경로가 긴 파일 {0}개 (Windows MAX_PATH 주의)', {0: p.longPaths.length})}</p>}
      {p.documentExists && (
        <p className="dialog-alert" role="alert">
          <Icon name="warning" size={16} />
          <span>{tr('이 프로젝트에 {0}이(가) 이미 있어 가져올 수 없습니다. 다른 이름을 쓰세요.', {0: p.document})}</span>
          {p.suggestion && <button onClick={() => onUseSuggestion(p.suggestion!)}>{tr('"{0}"(으)로 바꾸기', {0: p.suggestion})}</button>}
        </p>
      )}
      {p.warnings.map((w, i) => <p key={w} className="warnline small">{engineMessage({ message: w, ...p.warningMessages?.[i] })}</p>)}
    </section>
  );
}

// ---------------------------------------------------------------------------------------
// project explorer

export function ProjectExplorer({
  tree,
  current,
  onOpen,
  onRefresh,
  onImport,
  onNew,
}: {
  tree: ProjectTree;
  current: string | null;
  onOpen: (path: string) => void;
  onRefresh: () => void;
  onImport: () => void;
  /** File ▸ New ▸ Document… */
  onNew?: () => void;
}) {
  const docs = useMemo(() => new Map(tree.documents.map((d) => [d.path, d])), [tree]);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const toggle = (p: string) => setOpen((o) => ({ ...o, [p]: !o[p] }));
  const render = (e: TreeEntry, depth: number): React.ReactNode => {
    const pad = { paddingLeft: 8 + depth * 12 };
    if (e.kind === 'dir') {
      const isOpen = open[e.path] ?? depth === 0;
      return (
        <div key={e.path}>
          <button className="ex-row dir" style={pad} onClick={() => toggle(e.path)} title={e.path}>
            <span className="twisty">{isOpen ? '▾' : '▸'}</span>
            <span className="ex-name">{e.name}</span>
            <span className="n">{size(e.bytes)}</span>
          </button>
          {isOpen && e.children.map((c) => render(c, depth + 1))}
        </div>
      );
    }
    const d = e.kind === 'document' ? docs.get(e.path) : undefined;
    const missing = d ? d.links - d.linksPresent : 0;
    return (
      <button
        key={e.path}
        className={`ex-row ${e.kind}${e.path === current ? ' on' : ''}${e.conflict ? ' conflict' : ''}`}
        style={pad}
        onDoubleClick={() => e.kind === 'document' && onOpen(e.path)}
        title={e.conflict ? tr('동기화 충돌 사본 ({0}) — 다른 컴퓨터에서 동시에 고친 흔적입니다', {0:e.conflict}) : e.path}
      >
        <span className="twisty">{e.kind === 'document' ? '◆' : ''}</span>
        <span className="ex-name">{e.name}</span>
        {e.conflict && <span className="ex-badge bad">{tr("충돌")}</span>}
        {d?.lockedBy && !d.lockedBy.stale && !d.lockedBy.mine && <span className="ex-badge warn" title={`${d.lockedBy.user} @ ${d.lockedBy.machine}`}>{tr("사용 중")}</span>}
        {d && <span className={missing ? 'ex-badge warn' : 'ex-badge ok'} title={tr("링크: 프로젝트 안에 있음 / 전체 (없는 링크는 가져올 때 없던 것)")}>{d.linksPresent}/{d.links}</span>}
        {e.kind !== 'document' && <span className="n">{size(e.bytes)}</span>}
      </button>
    );
  };
  return (
    <aside className="explorer">
      <div className="ex-head">
        <b title={tree.root}>{tree.name}</b>
        {onNew && <button className="ex-icon" onClick={onNew} title={tr('새 문서…')}>▭</button>}
        <button className="ex-icon" onClick={onImport} title={tr("InDesign 문서 가져오기")}>＋</button>
        <button className="ex-icon" onClick={onRefresh} title={tr("새로 고침")}>↻</button>
      </div>
      {tree.conflicts.length > 0 && <p className="ex-alert bad">{tr('동기화 충돌 사본 {0}개', {0:tree.conflicts.length})}</p>}
      {tree.issues.length > 0 && <p className="ex-alert warn" title={tree.issues.map((i) => `${i.folder}/${i.name}: ${i.kind} ${i.detail ?? ''}`).join('\n')}>{tr('이름 문제 {0}개', {0:tree.issues.length})}</p>}
      <div className="ex-tree">{tree.entries.map((e) => render(e, 0))}</div>
      <p className="ex-foot muted small">{tr("문서를 두 번 눌러 엽니다")}</p>
    </aside>
  );
}

// ---------------------------------------------------------------------------------------
// read-only banner and title

export function LockBanner({ lock, onOverride }: { lock: LockState; onOverride: () => void }) {
  if (!lock.readOnly) return null;
  const who = lock.user ? `${lock.user} (${lock.machine})` : lock.machine || tr('다른 사용자');
  return (
    <div className="lock-banner">
      <b>{tr('읽기 전용')}</b>
      <span>{tr('{0}이(가) 이 문서를 열고 있습니다{1}. 저장할 수 없습니다.{2}', {0: who, 1: lock.time ? tr(' — {0} 열림', {0: age(lock.ageSecs)}) : '', 2: lock.sameMachine && tr(' (같은 컴퓨터)')})}</span>
      <button onClick={onOverride} title={tr('상대가 이미 닫았다고 확실할 때만 쓰세요. 두 사람이 동시에 저장하면 한쪽 변경이 사라집니다.')}>{tr('잠금 가져오기…')}</button>
    </div>
  );
}

export function useWindowTitle(project: ProjectInfo | null, folio: FolioSummary | null | undefined, docPath: string | null) {
  const language = useLanguage();
  useEffect(() => {
    const parts: string[] = [];
    if (docPath) parts.push(baseName(docPath) + (folio?.readOnly ? tr(' [읽기 전용]') : ''));
    const name = folio?.project.name ?? project?.name;
    if (name) parts.push(name);
    parts.push('Folio Studio');
    window.folioProject?.setTitle(parts.join(' — '));
  }, [project, folio, docPath, language]);
}

/** Recent projects from the app's data folder (machine settings, never in a project). */
export function useRecentProjects() {
  const [list, setList] = useState<RecentProject[]>([]);
  const refresh = useCallback(() => window.folioProject?.recent().then(setList, () => {}), []);
  useEffect(() => { refresh(); return window.folio.onEvent(e => { if(e.event === 'user-settings') refresh(); }); }, [refresh]);
  const remember = useCallback(async (p: RecentProject) => setList(await window.folioProject.remember(p)), []);
  return { recentProjects: list, rememberProject: remember, refreshRecentProjects: refresh };
}

// ---------------------------------------------------------------------------------------
// IDML export (principle 2: show what InDesign will not get)

export function IdmlExportDialog({ folio, onClose }: { folio: FolioSummary; onClose: () => void }) {
  const [state, setState] = useState<{ kind: 'idle' } | { kind: 'running' } | { kind: 'done'; path: string; report: IdmlExportReport } | { kind: 'error'; message: string }>({ kind: 'idle' });
  const run = async () => {
    const path = await window.folioProject.saveIdml(baseName(folio.document.path).replace(/\.folio$/i, '.idml'));
    if (!path) return;
    setState({ kind: 'running' });
    try {
      const report = await exportIdml(path);
      setState({ kind: 'done', path, report });
    } catch (e) {
      setState({ kind: 'error', message: (e as Error).message });
    }
  };
  return (
    <div className="overlay">
      <div className="card dialog">
        <h2>{tr('IDML로 내보내기 ')}<span>{tr('InDesign 교환')}</span></h2>
        <section>
          <p className="small">{tr('링크는 이 컴퓨터의 실제 파일 위치로, Folio 정보(고유 ID·상대 경로·해시)는 folio.* 스크립트 레이블로 실어 보냅니다. Folio는 INDD를 직접 쓰지 않으니, INDD가 필요하면 이 IDML을 InDesign에서 열어 저장하세요.')}</p>
        </section>
        {state.kind === 'done' && (
          <section>
            <h3>{tr('InDesign에서 달라지는 것')}</h3>
            <ul className="pkg-lines">
              {state.report.differences.filter((d) => d.count > 0).map((d) => <li key={d.what} className={d.what === 'indd' || d.what === 'projectData' ? '' : 'warn'}>{hasTranslation(`idml.${d.what}`) ? tr(`idml.${d.what}`) : d.effect} ({d.count})</li>)}
            </ul>
            <LimitsBlock limits={state.report.limits} />
            <p className="muted small" title={state.path}>{state.path} — {size(state.report.bytes)}</p>
          </section>
        )}
        {state.kind === 'error' && <p className="dialog-alert" role="alert"><Icon name="warning" size={16} /><span>{state.message}</span></p>}
        <div className="actions">
          <button onClick={onClose}>{tr('닫기')}</button>
          {state.kind !== 'done' && <button className="primary" disabled={state.kind === 'running'} onClick={run}>{state.kind === 'running' ? tr('내보내는 중…') : tr('내보내기…')}</button>}
        </div>
      </div>
    </div>
  );
}
