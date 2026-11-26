import { tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { SelEntry } from '../../ui/overlay';
import type { Command } from '../../ui/commands';
import { listMathExpressions, type MathExpression } from './api';

type Context = {
  editable: boolean;
  /** The page of the document window (its id). */
  page?: string;
  selection: SelEntry[];
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

/** What the Math Expressions panel does (PanelCtx.math). */
export type MathActions = {
  editable: boolean;
  /** The "Insert MathML…" dialog for a new equation. */
  insert: () => void;
  /** The same dialog for an equation of the document. */
  fix: (item: MathExpression) => void;
  remove: (item: MathExpression) => void;
};

/**
 * Object ▸ Insert MathML… (also in the context menu of an object) and Window ▸ Math Expressions. The dialog is one for both: a new equation,
 * or the selected equation (found by asking the engine which objects are equations). Everything the dialog does is an engine command
 * (`insertMathml`, `setMathml`), so a script does the same with the same values.
 */
export function useMath(ctx: Context) {
  // the engine session holds the dialog (`insertMath` on the page, `editMath` on an equation); the shared dialog frame shows it
  const open = (target?: MathExpression) => {
    const request = target ? { cmd: 'openDialog', dialog: 'editMath', ids: [target.id] } : { cmd: 'openDialog', dialog: 'insertMath', ids: [ctx.page ?? ''] };
    void window.folio.request(request).catch(() => {});
  };
  const insertMathml = async () => {
    // the selected object is an equation: change it; anything else: a new one
    const one = ctx.selection.length === 1 && !ctx.selection[0].content ? ctx.selection[0] : null;
    if (one) {
      const hit = (await listMathExpressions()).find((x) => x.id === one.id);
      if (hit) return open(hit);
    }
    open();
  };
  const handlers: Record<string, Partial<Command>> = {
    'id.object.insertMathml': { enabled: ctx.editable && !!ctx.page, run: () => void insertMathml() },
  };
  const actions: MathActions = {
    editable: ctx.editable && !!ctx.page,
    insert: () => open(),
    fix: (item) => open(item),
    remove: (item) => void ctx.edit(tr('수식 삭제'), [{ op: 'delete', ids: [item.id] }]),
  };
  return { handlers, actions };
}
