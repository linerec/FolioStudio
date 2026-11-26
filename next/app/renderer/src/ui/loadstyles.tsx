// Load Paragraph / Character / Object / All Text Styles… (the style panels' menus): a document (IDML or .folio) is chosen, its styles are listed
// with the ones this document already has marked, and the checked ones come in with `loadStyles` (engine edit/src/ops/loadstyles.rs, oracle
// `loadstyles` ls-*). The engine reads the file (query `stylesFile`) and hands back only the style and swatch definitions: no path of the other
// document is kept anywhere. Use Incoming Style Definition / Auto-Rename are InDesign's two answers for a name that exists.
import { createPortal } from 'react-dom';
import { useEffect, useState } from 'react';
import { t as tr } from '../i18n';
import type { PanelCtx } from './panels';

type FileStyle = { kind: 'paragraph' | 'character' | 'object' | 'cell' | 'table'; id: string; name: string; group: string[]; conflict: boolean; same: boolean };
type Kinds = FileStyle['kind'] | 'text' | 'tableCell';
/** The kinds the engine is asked for (Load Table and Cell Styles is both). */
const kindList = (k: Kinds) => (k === 'tableCell' ? ['table', 'cell'] : [k]);

export function LoadStylesDialog({ ctx, kinds, close }: { ctx: PanelCtx; kinds: Kinds; close: () => void }) {
  const [file, setFile] = useState<{ parts: Record<string, string>; styles: FileStyle[] } | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [clash, setClash] = useState<'overwrite' | 'rename'>('overwrite');
  const [error, setError] = useState('');
  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const path = await window.folioProject.openAny();
        if (!path) { if (live) close(); return; }
        const { header } = await window.folio.request({ cmd: 'stylesFile', path, kinds: kindList(kinds) });
        if (!header.ok) throw new Error(String(header.error));
        if (!live) return;
        const styles = header.styles as FileStyle[];
        setFile({ parts: header.parts as Record<string, string>, styles });
        setChecked(new Set(styles.map((s) => s.id)));
      } catch (e) {
        if (live) setError(String(e instanceof Error ? e.message : e));
      }
    })();
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const submit = () => {
    if (!file || checked.size === 0) return;
    ctx.edit(tr('스타일 불러오기'), [{ op: 'loadStyles', kinds: kindList(kinds), parts: file.parts, clash, only: [...checked] }]);
    close();
  };
  const toggle = (id: string) => setChecked((c) => { const n = new Set(c); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const anyClash = !!file?.styles.some((s) => checked.has(s.id) && s.conflict);
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('스타일 불러오기')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options load-styles" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <h2>{tr('스타일 불러오기')}</h2>
        {file && file.styles.length === 0 && <p>{tr('불러올 스타일이 없습니다.')}</p>}
        {file && file.styles.length > 0 && <>
          <div className="st-list" data-load-styles-list style={{ maxHeight: 260, overflow: 'auto' }}>
            {file.styles.map((s) => <label key={s.id} className="st-row" data-load-style={s.id}>
              <input type="checkbox" aria-label={s.name} checked={checked.has(s.id)} onChange={() => toggle(s.id)} />
              {[...s.group, s.name].join(' / ')}{s.conflict ? ` — ${tr('이름이 같은 스타일이 있습니다')}` : ''}
            </label>)}
          </div>
          <label>{tr('이름이 같은 스타일:')}
            <select aria-label={tr('이름이 같은 스타일')} value={clash} disabled={!anyClash} onChange={(e) => setClash(e.target.value as 'overwrite' | 'rename')}>
              <option value="overwrite">{tr('들어오는 스타일 정의 사용')}</option>
              <option value="rename">{tr('자동으로 이름 바꾸기')}</option>
            </select></label>
        </>}
        {error && <p role="alert">{error}</p>}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!file || checked.size === 0}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
