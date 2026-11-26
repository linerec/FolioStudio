// Window ▸ Output ▸ Attributes, Window ▸ Utilities ▸ Script Label, Tool Hints and Background Tasks.
// Attributes and Script Label edit the selected objects with the engine command `setObjectAttributes` (edit/src/ops/attributes.rs,
// oracle scenarios attrpanels.json); Tool Hints and Background Tasks are read-only lists.
import { useEffect, useState } from 'react';
import { t as tr } from '../i18n';
import type { ItemInfo } from '../inspect';
import { type PanelCtx } from './panels';
import { strokeTargets } from './stroke';
import { ALL_TOOLS } from './tools';

const SOLID = 'StrokeStyle/$ID/Solid';
type Flag = 'overprintFill' | 'overprintStroke' | 'overprintGap' | 'nonprinting';

const FLAGS: { prop: Flag; label: () => string; get: (i: ItemInfo) => boolean }[] = [
  { prop: 'overprintFill', label: () => tr('칠 겹쳐 찍기'), get: (i) => !!i.print?.overprint_fill },
  { prop: 'overprintStroke', label: () => tr('획 겹쳐 찍기'), get: (i) => !!i.print?.overprint_stroke },
  { prop: 'overprintGap', label: () => tr('간격 겹쳐 찍기'), get: (i) => !!i.print?.overprint_gap },
  { prop: 'nonprinting', label: () => tr('인쇄 안 함'), get: (i) => !!i.print?.nonprinting },
];

/** Whether the setting applies to the object (a group: to something inside it), as the engine decides (ops/attributes.rs). */
export function applies(item: ItemInfo, prop: Flag): boolean {
  if (item.kind === 'group') return item.children.some((c) => applies(c, prop));
  const stroked = item.stroke.kind !== 'none';
  if (prop === 'overprintFill') return item.kind !== 'line' && item.fill.kind !== 'none';
  if (prop === 'overprintStroke') return stroked;
  if (prop === 'overprintGap') return stroked && item.stroke.type !== SOLID && item.stroke.gap.kind !== 'none';
  return true;
}

/** The box's value for an object: a group's overprint is its members' (the file keeps it on the members), nonprinting is its own. */
function valueOf(item: ItemInfo, f: (typeof FLAGS)[number]): boolean {
  if (item.kind === 'group' && f.prop !== 'nonprinting') {
    const members = item.children.filter((c) => applies(c, f.prop));
    return members.length > 0 && members.every((c) => valueOf(c, f));
  }
  return f.get(item);
}

function useTick(ctx: PanelCtx) {
  const [rev, tick] = useState(0);
  useEffect(() => ctx.store.subscribe(() => tick((n) => n + 1)), [ctx.store]);
  return rev;
}

function Check({ prop, text, state, disabled, onToggle }: { prop: string; text: string; state: boolean | null; disabled: boolean; onToggle: () => void }) {
  return (
    <label className="p-row" style={{ display: 'flex', gap: 6, alignItems: 'center', opacity: disabled ? 0.5 : 1 }}>
      <input type="checkbox" data-attr={prop} checked={state === true} disabled={disabled} ref={(el) => { if (el) el.indeterminate = state === null; }} onChange={onToggle} />
      <span>{text}</span>
    </label>
  );
}

export function AttributesPanel({ ctx }: { ctx: PanelCtx }) {
  useTick(ctx);
  const targets = strokeTargets(ctx.selection, (pg) => ctx.store.peek(pg));
  if (!ctx.doc) return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  return (
    <div className="p-form attributes-panel" style={{ padding: 8 }}>
      {FLAGS.map((f) => {
        const on = targets.filter((t) => applies(t.item, f.prop));
        const values = on.map((t) => valueOf(t.item, f));
        const state = !values.length ? false : values.every((v) => v === values[0]) ? values[0] : null;
        const can = ctx.editable && on.length > 0;
        return <Check key={f.prop} prop={f.prop} text={f.label()} state={state} disabled={!can}
          onToggle={() => { if (can) void ctx.edit(f.label(), [{ op: 'setObjectAttributes', ids: on.map((t) => t.id), props: { [f.prop]: state !== true } }]); }} />;
      })}
      {!targets.length && <p className="hint">{tr('개체를 선택하면 겹쳐 찍기와 인쇄 안 함을 정할 수 있습니다.')}</p>}
    </div>
  );
}

export function ScriptLabelPanel({ ctx }: { ctx: PanelCtx }) {
  const rev = useTick(ctx);
  const targets = strokeTargets(ctx.selection, (pg) => ctx.store.peek(pg));
  const ids = targets.map((t) => t.id);
  const key = ids.join(',');
  const [shown, setShown] = useState<string>('');
  const [text, setText] = useState<string | null>(null);
  // what is being typed goes when another object is selected, not when an edit refreshes the document
  useEffect(() => { setText(null); }, [key]);
  useEffect(() => {
    if (ids.length !== 1) { setShown(''); return; }
    let alive = true;
    window.folio.request({ cmd: 'labels', ids }).then(({ header }) => { if (alive) setShown(String((header.labels as Record<string, string> | undefined)?.[ids[0]] ?? '')); }).catch(() => { if (alive) setShown(''); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, rev]);
  const can = ctx.editable && ids.length > 0;
  const commit = () => {
    if (text !== null && text !== shown && can) { setShown(text); void ctx.edit(tr('스크립트 레이블'), [{ op: 'setObjectAttributes', ids, props: { label: text } }]); }
    setText(null);
  };
  if (!ctx.doc) return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  return (
    <div className="p-form script-label-panel" style={{ padding: 8 }}>
      <label className="p-row" style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
        <span className="p-label">{tr('레이블:')}</span>
        <input className="p-input" data-label="" style={{ flex: 1 }} aria-label={tr('스크립트 레이블')} value={text ?? shown} disabled={!can}
          onChange={(e) => setText(e.target.value)} onBlur={commit}
          onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); commit(); } if (e.key === 'Escape') { e.preventDefault(); setText(null); e.currentTarget.blur(); } }} />
      </label>
      {ids.length > 1 && <p className="hint">{tr('여러 개체를 골랐습니다. 입력하면 모두에 같은 레이블이 붙습니다.')}</p>}
    </div>
  );
}

/** Tool Hints: the current tool, its shortcut and whether it works yet; read-only. */
export function ToolHintsPanel({ tool }: { tool?: string }) {
  const def = ALL_TOOLS.find((t) => t.id === tool);
  if (!def) return <p className="p-empty">{tr('도구를 고르면 이름과 단축키가 나옵니다.')}</p>;
  return (
    <div className="p-form tool-hints-panel" style={{ padding: 8 }}>
      <h3 data-tool={def.id}>{def.label}</h3>
      <p>{def.key ? tr('단축키: {0}', { 0: def.key }) : tr('단축키가 없습니다.')}</p>
    </div>
  );
}

/** Background Tasks: what runs behind the document now; read-only. */
export function BackgroundTasksPanel({ ctx }: { ctx: PanelCtx }) {
  const tasks = [ctx.linksBusy ? tr('링크 상태 확인') : null].filter((t): t is string => !!t);
  return (
    <div className="p-form background-tasks-panel" style={{ padding: 8 }}>
      {tasks.length ? <ul data-tasks="">{tasks.map((t) => <li key={t}>{t}</li>)}</ul> : <p className="p-empty">{tr('실행 중인 백그라운드 작업이 없습니다.')}</p>}
    </div>
  );
}
