import { useCallback, useEffect, useRef, useState } from 'react';
import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { listMathExpressions, mathSummary, type MathExpression } from './api';
import './panel.css';

// Window ▸ Math Expressions: the equations of the document, to select, change, delete, and the way to insert a new one. The list is the
// engine's (query `mathExpressions`); changing and inserting use the dialog of Object ▸ Insert MathML… (useMath.tsx).
export const MATH_PANELS = {
  mathExpressions: { get title() { return tr('수식'); }, icon: 'info', width: 300, height: 320 },
};

export function MathPanel({ ctx }: { ctx: PanelCtx }) {
  const math = ctx.math;
  const [items, setItems] = useState<MathExpression[] | null>(null);
  const serial = useRef(0);
  const refresh = useCallback(() => {
    const mine = ++serial.current;
    void listMathExpressions().then((rows) => { if (serial.current === mine) setItems(rows); });
  }, []);
  // the list again whenever the document changes (an equation inserted, changed, deleted, an edit undone) or another document is open
  useEffect(() => { if (ctx.doc) refresh(); else setItems(null); }, [ctx.info, ctx.doc?.path, refresh]);

  if (!ctx.doc || !math) return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  const pageOf = (item: MathExpression) => ctx.info?.pages.find((p) => p.id === item.page);
  const chosen = items?.find((x) => ctx.selection.some((s) => !s.content && s.id === x.id)) ?? null;
  const select = (item: MathExpression) => {
    const page = pageOf(item);
    if (!page) return;
    ctx.goToPage(page.index);
    ctx.setSelection([{ page: page.index, id: item.id, content: false }]);
  };
  return (
    <div className="panel-body math-panel" data-math-panel>
      <div className="math-panel-actions">
        <button disabled={!math.editable} onClick={math.insert}>{tr('삽입…')}</button>
        <button disabled={!math.editable || !chosen} onClick={() => chosen && math.fix(chosen)}>{tr('고치기…')}</button>
        <button disabled={!math.editable || !chosen} onClick={() => chosen && math.remove(chosen)}>{tr('지우기')}</button>
      </div>
      {items && items.length === 0 && <p className="p-empty">{tr('이 문서에는 수식이 없습니다.')}</p>}
      {!!items?.length && (
        <ul role="listbox" aria-label={tr('수식')}>
          {items.map((item) => (
            <li key={item.id} role="option" aria-selected={chosen?.id === item.id} data-math-expression={item.id} onClick={() => select(item)} onDoubleClick={() => math.editable && math.fix(item)}>
              <span className="math-source">{mathSummary(item.mathml)}</span>
              <span className="math-meta">{tr('{0}쪽 · {1}pt', { 0: (pageOf(item)?.index ?? 0) + 1, 1: item.size })}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
