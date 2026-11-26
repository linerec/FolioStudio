// File ▸ New ▸ Document… is the engine's app dialog (next/shared/dialogs/newDocument.json: page size and orientation, pages, facing
// pages, margins, columns, bleed and slug, units, an installed font, the document preset it starts from); the engine writes the
// document (folio-project/src/newdoc.rs, measured against InDesign in next/oracle/newdoc/) into the open project, named
// "제목 없음-N". What stays here: the notice when no project is open, and the list of document presets (File ▸ Document Presets ▸
// Define…), whose New and Edit open the engine's preset editor (documentPreset.json) and whose Delete is the engine's request.
import { useState } from 'react';
import { tr } from '../i18n';
import { deleteDocumentPreset, openDocumentPreset } from '../project';
import type { DocumentPreset } from './docpresets';
import './newdoc.css';

/** File ▸ New ▸ Document… with no project open: documents are made in a project folder. */
export function NewDocumentNotice({ onClose, openProject, newProject }: { onClose: () => void; openProject: () => void; newProject: () => void }) {
  const title = tr('새 문서');
  return <div className="overlay page-dialog-scrim" onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') onClose(); }}>
    <div className="modal page-dialog" role="dialog" aria-modal="true" aria-label={title} data-newdoc-notice>
      <h2>{title}</h2>
      <p>{tr('문서는 프로젝트 폴더 안에 만듭니다. 프로젝트를 열거나 새로 만든 다음 다시 선택하세요.')}</p>
      <div className="actions">
        <button type="button" onClick={onClose}>{tr('취소')}</button>
        <button type="button" onClick={() => { onClose(); openProject(); }}>{tr('프로젝트 열기…')}</button>
        <button type="button" className="primary" onClick={() => { onClose(); newProject(); }}>{tr('새 프로젝트…')}</button>
      </div>
    </div>
  </div>;
}

/** File ▸ Document Presets ▸ Define…: the presets, to add, edit and delete ([Default] stays). The list is the user's settings. */
export function DocumentPresetsDialog({ presets, changed, onClose }: { presets: DocumentPreset[]; changed: () => void; onClose: () => void }) {
  const [pick, setPick] = useState<string | null>(null), [error, setError] = useState('');
  const chosen = presets.find((p) => p.name === pick) ?? null;
  // (a deleted preset: the list is the app's settings, read again)
  const ask = (request: Promise<{ header: Record<string, unknown> }>) => void request.then(({ header }) => {
    setError(header.ok ? '' : String(header.error));
    if (header.ok && header.type === 'documentPresets') changed();
  });
  return <div className="overlay page-dialog-scrim" onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') onClose(); }}>
    <div className="modal page-dialog" role="dialog" aria-modal="true" aria-label={tr('문서 사전 설정')}>
      <h2>{tr('문서 사전 설정')}</h2>
      <ul className="docpresets" data-field="presetList">
        <li className="muted">{tr('[기본값]')}</li>
        {presets.map((p) => <li key={p.name}><button type="button" className={p.name === pick ? 'on' : ''} onClick={() => setPick(p.name)} onDoubleClick={() => ask(openDocumentPreset(p.name))}>{p.name}</button></li>)}
      </ul>
      {error && <p role="alert" className="error">{error}</p>}
      <div className="actions">
        <button type="button" onClick={() => ask(openDocumentPreset())} data-field="presetNew">{tr('새로 만들기…')}</button>
        <button type="button" disabled={!chosen} onClick={() => chosen && ask(openDocumentPreset(chosen.name))} data-field="presetEdit">{tr('편집…')}</button>
        <button type="button" disabled={!chosen} onClick={() => { if (chosen) { ask(deleteDocumentPreset(chosen.name)); setPick(null); } }} data-field="presetDelete">{tr('삭제')}</button>
        <button type="button" className="primary" onClick={onClose}>{tr('확인')}</button>
      </div>
    </div>
  </div>;
}
