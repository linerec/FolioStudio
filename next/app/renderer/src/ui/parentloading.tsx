import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';

export function ParentLoadingDialog({ edit, close }: { edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>; close: () => void }) {
  const [parts, setParts] = useState<Record<string, string> | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const path = await window.folioProject.openAny();
        if (!path) { if (live) close(); return; }
        const { header } = await window.folio.request({ cmd: 'parentsFile', path });
        if (!header.ok) throw Error(String(header.error));
        if (live) setParts(header.parts as Record<string, string>);
      } catch (e) { if (live) setError(String(e)); }
    })();
    return () => { live = false; };
  }, []);
  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('상위 페이지 불러오기')} onPointerDown={e => e.stopPropagation()} onKeyDown={e => { e.stopPropagation(); if (e.key === 'Escape' && !busy) close(); }}>
    <form className="card dialog parent-loading" onSubmit={async e => {
      e.preventDefault(); if (!parts || busy) return;
      setBusy(true);
      try { const result = await edit(tr('상위 페이지 불러오기'), [{ op: 'loadParentPages', parts }]); if (result) close(); }
      catch (e) { setError(String(e)); }
      finally { setBusy(false); }
    }}>
      <h2>{tr('상위 페이지 불러오기')}</h2>
      <p>{tr('같은 이름이 있으면 가져온 상위 페이지의 이름을 바꿉니다.')}</p>
      {error && <p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={!parts || busy}>{tr('확인')}</button></div>
    </form>
  </div>, document.body);
}
