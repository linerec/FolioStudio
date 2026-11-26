import { useCallback, useEffect, useState } from 'react';
import { t as tr } from '../i18n';
import { findText, packagePlan, type EditCommand, type EditResult, type PackagePlan } from '../engine';
import { fontFamilyProperties } from './fontfamily';
import { fieldKey } from './keys';
import './typefontfind.css';

export type FontFindContext = {
  documentPath?: string; editable: boolean; roots: string[];
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};
type FontRow = PackagePlan['preview']['fonts'][number];
const fontKey = (f: FontRow) => JSON.stringify([f.family, f.style]);

export function TypeFontFindDialog({ context, fonts, close }: {
  context: FontFindContext; fonts: Record<string, string[]>; close: () => void;
}) {
  const [rows, setRows] = useState<FontRow[]>([]);
  const [selected, setSelected] = useState('');
  const [family, setFamily] = useState('');
  const [style, setStyle] = useState('');
  const [busy, setBusy] = useState(true);
  const [status, setStatus] = useState('');
  const names = Object.keys(fonts).sort((a, b) => a.localeCompare(b));
  const source = rows.find(f => fontKey(f) === selected);
  const refresh = useCallback(async () => {
    const plan = await packagePlan({ links: false, fonts: false, pdf: false, cjkFonts: false, roots: context.roots });
    const list = plan.preview.fonts;
    setRows(list);
    setSelected(old => list.some(f => fontKey(f) === old) ? old : list[0] ? fontKey(list[0]) : '');
  }, [context.roots]);
  useEffect(() => {
    void refresh().catch(e => setStatus(String(e))).finally(() => setBusy(false));
  }, [refresh]);
  const changeFamily = (name: string) => {
    setFamily(name);
    setStyle(String(fontFamilyProperties(fonts, name, style).fontStyle));
  };
  const canReplace = context.editable && !busy && !!source && !!family && fonts[family]?.includes(style)
    && (source.family !== family || source.style !== style);
  const replaceAll = async () => {
    if (!canReplace || !source) return;
    setBusy(true);
    setStatus('');
    try {
      const query = { text: '', scope: 'document' as const, props: { appliedFont: source.family, fontStyle: source.style } };
      const { matches } = await findText(query);
      if (!matches.length) { setStatus(tr('찾은 글이 없습니다.')); return; }
      const result = await context.edit(tr('글꼴 바꾸기'), [{ op: 'findReplace', query, props: { appliedFont: family, fontStyle: style } }]);
      if (result) {
        await refresh();
        setStatus(tr('{0}곳을 바꿨습니다.', { 0: matches.length }));
      } else setStatus(tr('글꼴을 바꾸지 못했습니다.'));
    } catch (e) { setStatus(String(e)); } finally { setBusy(false); }
  };
  return <div className="style-modal" role="dialog" aria-modal="true" aria-label={tr('문서 글꼴 찾기')}
    onPointerDown={e => e.stopPropagation()} onKeyDown={e => {
      e.stopPropagation();
      if (fieldKey(e) === 'cancel' && !busy) { e.preventDefault(); close(); }
    }}>
    <form className="card dialog style-options type-font-find" onSubmit={e => { e.preventDefault(); void replaceAll(); }}>
      <h2>{tr('문서 글꼴 찾기')}</h2>
      <div className="type-font-find-list">
        <table><thead><tr><th>{tr('글꼴')}</th><th>{tr('글꼴 스타일')}</th><th>{tr('설치 상태')}</th></tr></thead>
          <tbody>{rows.map(row => <tr key={fontKey(row)} data-font-family={row.family} data-font-style={row.style}>
            <td><label><input type="radio" name="document-font" value={fontKey(row)} checked={selected === fontKey(row)}
              disabled={busy} onChange={() => setSelected(fontKey(row))} />{row.family}</label></td>
            <td>{row.style}</td><td title={row.substitute ?? undefined}>{row.status === 'missing' || row.status === 'substituted' ? tr('없음') : tr('설치됨')}</td>
          </tr>)}</tbody>
        </table>
        {!busy && !rows.length && <p>{tr('문서에 사용한 글꼴이 없습니다.')}</p>}
      </div>
      <label>{tr('바꿀 글꼴')}<select aria-label={tr('바꿀 글꼴')} disabled={busy || !context.editable} value={family} onChange={e => changeFamily(e.target.value)}>
        <option value="">{tr('글꼴을 선택하세요.')}</option>{names.map(name => <option key={name} value={name}>{name}</option>)}
      </select></label>
      <label>{tr('바꿀 글꼴 스타일')}<select aria-label={tr('바꿀 글꼴 스타일')} disabled={busy || !context.editable || !family} value={style} onChange={e => setStyle(e.target.value)}>
        {!family && <option value="" />}{(fonts[family] ?? []).map(name => <option key={name} value={name}>{name}</option>)}
      </select></label>
      <p>{tr('문서의 글 내용에서 선택한 가족과 스타일을 모두 바꿉니다. 스타일 정의는 바꾸지 않습니다.')}</p>
      <p role="status">{busy ? tr('글꼴을 확인하는 중…') : status}</p>
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('닫기')}</button>
        <button type="submit" disabled={!canReplace}>{tr('모두 바꾸기')}</button></div>
    </form>
  </div>;
}
