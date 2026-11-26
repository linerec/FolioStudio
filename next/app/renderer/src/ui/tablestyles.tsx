// Window ▸ Styles ▸ Cell Styles and Table Styles: the panels (a click puts the style on the selected cells or on the table; Alt clears
// the overrides too) and the style dialog (new, options, delete). The engine writes and applies them as InDesign does
// (edit/src/ops/tables/styles.rs and tablestyledefs.rs, oracle tablestyles). A style's settings are the IDML attributes of its element, so the
// dialog edits them as the file has them; a table style's alternating patterns reuse the Table Options tabs (tablepatterns.tsx).
import { t as tr } from '../i18n';
import { useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { type EditCommand, type TableInfo, tableInfo } from '../engine';
import type { DocumentInfo, StyleGroupInfo, TableStyleInfo } from '../inspect';
import { MoveDialog, NameDialog, emptyGroup, groupName, styleRows } from './tablestylegroups';
import { FillPatterns, type PatternTab, type PatternValues, StrokePatterns, patternChanges, patternsValid } from './tablepatterns';
import type { PanelCtx } from './panels';
import type { TableSelection } from './textedit';
import { LoadStylesDialog } from './loadstyles';
import { GroupPickDialog } from './objectstylegroups';
import { StyleRowMenu, useRowMenu } from './stylecontext';
import { type TableStyleAction, tableStyleRowActions } from './tablestylemenu';
import { TextNumberField, formatNumeric, parseNumeric } from './controls/lengthfield';

type Kind = 'cell' | 'table';
type Mode = 'create' | 'edit' | 'delete';
const NONE: Record<Kind, string> = { cell: 'CellStyle/$ID/[None]', table: 'TableStyle/$ID/[No table style]' };
const NO_PARAGRAPH = 'ParagraphStyle/$ID/[No paragraph style]';
const SIDES = ['Top', 'Left', 'Bottom', 'Right'] as const;
const sideName = () => ({ Top: tr('위쪽'), Left: tr('왼쪽'), Bottom: tr('아래쪽'), Right: tr('오른쪽') });
const regionName = () => ({ Header: tr('머리글 행'), Footer: tr('바닥글 행'), LeftColumn: tr('왼쪽 열'), RightColumn: tr('오른쪽 열'), Body: tr('본문 행') });

/** A style's name with its groups before it. */
const pathName = (s: TableStyleInfo) => [...(s.group ?? []), s.name].join(' / ');

/** The styles by name, the built-in ones first (Sort by Name: the list only). */
const byName = (list: TableStyleInfo[]) => [...list].sort((a, b) => Number(b.id.includes('/$ID/')) - Number(a.id.includes('/$ID/')) || a.name.localeCompare(b.name));

const listOf = (info: DocumentInfo | null, kind: Kind) => (kind === 'cell' ? info?.cellStyles : info?.tableStyles) ?? [];

/** The cells ([column, row]) the selection covers. */
function selectedCells(sel: TableSelection): [number, number][] {
  const [c0, r0] = sel.from, [c1, r1] = sel.to;
  return sel.info.cells.filter((c) => c.cell[0] >= c0 && c.cell[0] <= c1 && c.cell[1] >= r0 && c.cell[1] <= r1).map((c) => c.cell);
}

/** What a style gives: its own settings over those of the styles it is based on (nearest wins); the defaults under them. */
function effective(list: TableStyleInfo[], id: string, defaults: Record<string, string> = {}): Record<string, string> {
  const chain: TableStyleInfo[] = [];
  const seen = new Set<string>();
  for (let at = id; at && !seen.has(at); ) {
    seen.add(at);
    const s = list.find((x) => x.id === at);
    if (!s) break;
    chain.push(s);
    at = s.basedOn;
  }
  return Object.assign({}, defaults, ...chain.reverse().map((s) => s.settings));
}

export function TableStylesPanel({ ctx, kind }: { ctx: PanelCtx; kind: Kind }) {
  const list = listOf(ctx.info, kind);
  const sel = ctx.tableSelection ?? null;
  const [picked, setPicked] = useState('');
  const [dialog, setDialog] = useState<{ mode: Mode; style: TableStyleInfo | null; together?: string[] } | null>(null);
  // style groups: the group picked in the list, the shut ones, the panel menu and the small dialogs it opens
  const groups = (kind === 'cell' ? ctx.info?.cellStyleGroups : ctx.info?.tableStyleGroups) ?? [];
  const [pickedGroup, setPickedGroup] = useState('');
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState(false);
  const [small, setSmall] = useState(false);
  const [load, setLoad] = useState<false | 'one' | 'both'>(false);
  const [sub, setSub] = useState<'newGroup' | 'renameGroup' | 'duplicate' | 'move' | 'selection' | 'copyTo' | 'groupFromStyles' | null>(null);
  // Sort by Name (the list only), the styles Select All Unused picked, a refused copy
  const [sorted, setSorted] = useState(false);
  const [unused, setUnused] = useState<string[]>([]);
  const [error, setError] = useState('');
  const rowMenu = useRowMenu();
  // the table as it is now: the selection's own copy was taken when the cells were selected, before the edits since
  const [live, setLive] = useState<TableInfo | null>(null);
  const revision = (ctx.history?.undo.length ?? 0) * 1000 + (ctx.history?.redo.length ?? 0);
  useEffect(() => {
    let current = true;
    if (!sel) { setLive(null); return; }
    tableInfo(sel.table).then((r) => { if (current) setLive(r.table); }, () => { if (current) setLive(null); });
    return () => { current = false; };
  }, [sel?.table, revision]); // eslint-disable-line react-hooks/exhaustive-deps
  const shown = live && sel && live.id === sel.info.id ? live : sel?.info;
  const anchor = shown?.cells.find((c) => c.cell[0] === sel?.from[0] && c.cell[1] === sel?.from[1]) ?? shown?.cells[0];
  const applied = kind === 'table' ? (shown?.style ?? NONE.table) : (anchor?.style ?? NONE.cell);
  const current = list.find((s) => s.id === picked) ?? null;
  const locked = !ctx.editable;
  const builtIn = (id: string) => id.includes('/$ID/');
  const apply = (id: string, clear: boolean) => {
    if (!sel) return;
    const cells = selectedCells(sel);
    const out: EditCommand[] = kind === 'table'
      ? [{ op: 'applyTableStyle', table: sel.table, style: id }, ...(clear ? [{ op: 'clearTableOverrides', table: sel.table } as EditCommand] : [])]
      : [{ op: 'applyCellStyle', table: sel.table, cells, style: id }, ...(clear ? [{ op: 'clearCellOverrides', table: sel.table, cells } as EditCommand] : [])];
    ctx.edit(kind === 'table' ? tr('표 스타일 적용') : tr('셀 스타일 적용'), out);
  };
  const tag = kind === 'cell' ? 'CellStyle' : 'TableStyle';
  const group = groups.find((g) => g.id === pickedGroup) ?? null;
  const styleId = (path: string[], name: string) => `${tag}/${[...path, name].join('%3a')}`;
  const groupId = (path: string[]) => `${tag}Group/$ID/${path.join('%3a')}`;
  const sameName = (path: string[], name: string) => list.some((s) => (s.group ?? []).join('/') === path.join('/') && s.name === name);
  const ownable = !!current && !builtIn(current.id);
  const newGroup = (name: string) => {
    ctx.edit(tr('새 스타일 그룹'), [{ op: 'createTableStyleGroup', kind, name, ...(group ? { parent: group.id } : {}) }]);
    setPickedGroup(groupId([...(group?.path ?? []), name]));
    setPicked('');
  };
  const renameGroup = (name: string) => {
    if (!group) return;
    ctx.edit(tr('스타일 그룹 이름 바꾸기'), [{ op: 'renameTableStyleGroup', kind, group: group.id, name }]);
    setPickedGroup(groupId([...group.path.slice(0, -1), name]));
  };
  const deleteGroup = () => {
    if (!group) return;
    ctx.edit(tr('스타일 그룹 삭제'), [{ op: 'deleteTableStyleGroup', kind, group: group.id }]);
    setPickedGroup('');
  };
  const duplicate = (name: string) => {
    if (!current) return;
    ctx.edit(tr('스타일 복제'), [{ op: 'duplicateTableStyle', kind, style: current.id, name }]);
    setPicked(styleId(current.group ?? [], name));
  };
  const move = (to: string) => {
    if (!current) return;
    const path = groups.find((g) => g.id === to)?.path ?? [];
    ctx.edit(tr('스타일 그룹으로 옮기기'), [{ op: 'moveTableStyle', kind, style: current.id, ...(to ? { group: to } : {}) }]);
    setPicked(styleId(path, current.name));
  };
  // Redefine Style (cell styles): the style takes what the selected cell sets itself, then the cell is given the style with nothing over it
  // (the edit, apply and clear of the panel, each measured: oracle tablestyles cs-edit, cs-apply-*, cs-clear-*)
  const redefine = async () => {
    if (!sel || !current || builtIn(current.id)) return;
    if (kind === 'table') {
      ctx.edit(tr('스타일 재정의'), [{ op: 'redefineTableStyle', table: sel.table }]);
      return;
    }
    // the cell as the engine has it now (the panel's copy may be a moment behind)
    const fresh = await tableInfo(sel.table).then((r) => r.table, () => null);
    const anchor = (fresh ?? shown)?.cells.find((c) => c.cell[0] === sel.from[0] && c.cell[1] === sel.from[1]);
    if (!anchor) return;
    const eff = effective(list, current.id);
    const props: Record<string, string> = {};
    const put = (key: string, value: string | number | null | undefined) => { if (value != null && value !== '' && eff[key] !== String(value)) props[key] = String(value); };
    put('FillColor', anchor.fill);
    put('FillTint', anchor.fillTint);
    if (anchor.vertical !== 'TopAlign') put('VerticalJustification', anchor.vertical);
    SIDES.forEach((side, i) => {
      if (anchor.insets[i] != null && anchor.insets[i] !== 4) put(`${side}Inset`, anchor.insets[i]);
      put(`${side}EdgeStrokeWeight`, anchor.strokes[side].weight);
      put(`${side}EdgeStrokeColor`, anchor.strokes[side].color);
    });
    const cells = selectedCells(sel);
    ctx.edit(tr('스타일 재정의'), [
      ...(Object.keys(props).length ? [{ op: 'editTableStyle', kind, style: current.id, props } as EditCommand] : []),
      { op: 'applyCellStyle', table: sel.table, cells, style: current.id },
      { op: 'clearCellOverrides', table: sel.table, cells },
    ]);
  };
  const clear = () => {
    if (!sel) return;
    if (kind === 'table') ctx.edit(tr('표 스타일 재정의 지우기'), [{ op: 'clearTableOverrides', table: sel.table }]);
    else ctx.edit(tr('셀 스타일 재정의 지우기'), [{ op: 'clearCellOverrides', table: sel.table, cells: selectedCells(sel) }]);
  };
  // Break Link to Style (both panels) and Clear Attributes Not Defined by Style (cells): engine commands on the selection
  const breakLink = () => {
    if (!sel) return;
    if (kind === 'table') ctx.edit(tr('스타일에 대한 연결 끊기'), [{ op: 'breakTableStyleLink', table: sel.table }]);
    else ctx.edit(tr('스타일에 대한 연결 끊기'), [{ op: 'breakCellStyleLink', table: sel.table, cells: selectedCells(sel) }]);
  };
  const clearNotDefined = () => {
    if (!sel || kind !== 'cell') return;
    ctx.edit(tr('스타일에 정의되지 않은 특성 지우기'), [{ op: 'clearCellStyleAttributes', table: sel.table, cells: selectedCells(sel) }]);
  };
  // Copy to Group: the style stays and a copy with its name goes into the group (duplicate, move, then the name back; one undo step)
  const copyToGroup = (to: string) => {
    if (!current) return;
    const path = groups.find((g) => g.id === to)?.path ?? [];
    if (list.some((s) => s.id === styleId(path, current.name))) { setError(tr('같은 이름의 스타일이 이미 있습니다.')); return; }
    setError('');
    let temp = `${current.name} copy`;
    for (let n = 2; sameName(current.group ?? [], temp); n++) temp = `${current.name} copy ${n}`;
    ctx.edit(tr('그룹에 복사'), [
      { op: 'duplicateTableStyle', kind, style: current.id, name: temp },
      { op: 'moveTableStyle', kind, style: styleId(current.group ?? [], temp), group: to },
      { op: 'editTableStyle', kind, style: styleId(path, temp), rename: current.name, props: {} },
    ]);
  };
  // New Group from Styles: a group made new at the top gets the picked style (the panel picks one style at a time)
  const groupFromStyles = (name: string) => {
    if (!current) return;
    ctx.edit(tr('스타일에서 새 그룹'), [{ op: 'createTableStyleGroup', kind, name }, { op: 'moveTableStyle', kind, style: current.id, group: groupId([name]) }]);
    setPicked(styleId([name], current.name));
  };
  // Apply Style and what goes with it in the right-click menu: the overrides, and (tables) the cell styles of the table's cells
  const applyAlso = (clearOverrides: boolean, clearCellStyles: boolean) => {
    if (!sel || !current) return;
    const extra: EditCommand[] = clearCellStyles ? [{ op: 'applyCellStyle', table: sel.table, cells: (shown?.cells ?? []).map((c) => c.cell), style: NONE.cell }] : [];
    if (kind === 'table') {
      ctx.edit(tr('표 스타일 적용'), [{ op: 'applyTableStyle', table: sel.table, style: current.id }, ...(clearOverrides ? [{ op: 'clearTableOverrides', table: sel.table } as EditCommand] : []), ...extra]);
    } else apply(current.id, clearOverrides);
  };
  const unusedStyles = list.filter((s) => s.used === false && !builtIn(s.id)).map((s) => s.id);
  const actions: TableStyleAction[] = [
    { id: 'new', disabled: locked, run: () => setDialog({ mode: 'create', style: null }) },
    { id: 'options', disabled: locked || !current || builtIn(current.id), run: () => current && setDialog({ mode: 'edit', style: current }) },
    { id: 'duplicate', disabled: locked || !ownable, run: () => setSub('duplicate') },
    { id: 'delete', disabled: locked || !current || builtIn(current.id), run: () => current && setDialog({ mode: 'delete', style: current, ...(unused.length > 1 && unused.includes(current.id) ? { together: unused } : {}) }) },
    { id: 'redefine', disabled: locked || !ownable || !sel, run: () => void redefine() },
    { id: 'clear', disabled: locked || !sel, run: clear },
    { id: 'breakLink', disabled: locked || !sel, run: breakLink },
    ...(kind === 'cell' ? [{ id: 'clearNotDefined', disabled: locked || !sel, run: clearNotDefined }] : []),
    { id: 'load', disabled: locked, run: () => setLoad('one') },
    { id: 'loadBoth', disabled: locked, run: () => setLoad('both') },
    { id: 'selectUnused', disabled: unusedStyles.length === 0, run: () => { setUnused(unusedStyles); setPicked(unusedStyles[0] ?? ''); setPickedGroup(''); setClosed(new Set()); } },
    { id: 'newGroup', disabled: locked, run: () => setSub('newGroup') },
    { id: 'openAll', disabled: false, run: () => setClosed(new Set()) },
    { id: 'closeAll', disabled: false, run: () => setClosed(new Set(groups.map((g) => g.id))) },
    { id: 'copyToGroup', disabled: locked || !ownable || groups.length === 0, run: () => setSub('copyTo') },
    { id: 'newGroupFromStyles', disabled: locked || !ownable, run: () => setSub('groupFromStyles') },
    { id: 'sort', disabled: list.length < 2, run: () => setSorted(true) },
    { id: 'small', disabled: false, checked: small, run: () => setSmall((v) => !v) },
    // the row's right-click menu
    { id: 'apply', disabled: locked || !sel || !current || !!group, run: () => current && apply(current.id, false) },
    { id: 'applyClear', disabled: locked || !sel || !current || !!group, run: () => current && apply(current.id, true) },
    { id: 'applyClearCells', disabled: locked || !sel || !current || !!group, run: () => applyAlso(false, true) },
    { id: 'applyAll', disabled: locked || !sel || !current || !!group, run: () => (kind === 'table' ? applyAlso(true, true) : applyAlso(true, false)) },
    { id: 'duplicateGroup', disabled: locked || !group, run: () => group && ctx.edit(tr('스타일 그룹 복제'), [{ op: 'duplicateStyleGroup', kind, group: group.id }]) },
    { id: 'groupOptions', disabled: locked || !group, run: () => setSub('renameGroup') },
    { id: 'deleteGroup', disabled: locked || !group || (kind === 'table' && !emptyGroup(group, groups, list)), run: deleteGroup },
  ];
  useEffect(() => { ctx.tableStyleActions?.(kind, actions); });
  useEffect(() => () => ctx.tableStyleActions?.(kind, []), [ctx.tableStyleActions, kind]);
  const menuItem = (label: string, disabled: boolean, run: () => void) => (
    <button key={label} role="menuitem" disabled={disabled || locked} onClick={() => { setMenu(false); run(); }}>{label}</button>
  );
  return (
    <div className="styles-panel" data-table-styles={kind}>
      <div className={small ? 'st-list small-rows' : 'st-list'}>
        {styleRows(sorted ? byName(list) : list, sorted ? [...groups].sort((a, b) => groupName(a).localeCompare(groupName(b))) : groups, closed,
          (s, depth) => (
            <button key={s.id} className="st-row" style={{ paddingLeft: 10 + depth * 14 }} disabled={locked || !sel} data-table-style={s.id} aria-pressed={applied === s.id || unused.includes(s.id)}
              onContextMenu={(e) => { setPicked(s.id); setPickedGroup(''); setUnused([]); rowMenu.open(e); }}
              title={tr('{0} · 두 번 누르면 옵션', { 0: s.name })}
              onClick={(e) => { setPicked(s.id); setPickedGroup(''); setUnused([]); apply(s.id, e.altKey); }}
              onDoubleClick={() => { setPicked(s.id); if (!builtIn(s.id)) setDialog({ mode: 'edit', style: s }); }}>
              {s.name}
            </button>
          ),
          (g: StyleGroupInfo, depth, open) => (
            <button key={g.id} className="st-row st-group" style={{ paddingLeft: 10 + depth * 14 }} data-style-group={g.id} aria-expanded={open} aria-pressed={pickedGroup === g.id}
              onContextMenu={(e) => { setPickedGroup(g.id); setPicked(''); setUnused([]); rowMenu.open(e); }}
              onClick={() => { setPickedGroup(g.id); setPicked(''); setClosed((c) => { const n = new Set(c); if (n.has(g.id)) n.delete(g.id); else n.add(g.id); return n; }); }}>
              {open ? '▾ ' : '▸ '}{groupName(g)}
            </button>
          ))}
      </div>
      <div className="p-foot style-actions">
        <button disabled={locked} onClick={() => setDialog({ mode: 'create', style: null })}>{tr('새 스타일')}</button>
        <button disabled={locked || !current || builtIn(current.id)} onClick={() => current && setDialog({ mode: 'edit', style: current })}>{tr('옵션')}</button>
        <button disabled={locked || !sel} onClick={clear}>{tr('재정의 지우기')}</button>
        <button disabled={locked || !current || builtIn(current.id)} onClick={() => current && setDialog({ mode: 'delete', style: current, ...(unused.length > 1 && unused.includes(current.id) ? { together: unused } : {}) })}>{tr('삭제')}</button>
        <button aria-haspopup="menu" aria-expanded={menu} aria-label={tr('패널 메뉴')} onClick={() => setMenu((m) => !m)}>{'☰'}</button>
      </div>
      {menu && (
        <div className="st-menu" role="menu" aria-label={tr('패널 메뉴')} onKeyDown={(e) => { if (e.key === 'Escape') setMenu(false); }}>
          {menuItem(tr('선택에서 새 스타일…'), !sel || (kind === 'cell' && selectedCells(sel).length !== 1), () => setSub('selection'))}
          {menuItem(tr('스타일 불러오기…'), false, () => setLoad('one'))}
          {menuItem(tr('새 스타일 그룹…'), false, () => setSub('newGroup'))}
          {menuItem(tr('스타일 복제…'), !ownable, () => setSub('duplicate'))}
          {menuItem(tr('스타일 재정의'), !ownable || !sel, () => void redefine())}
          {menuItem(tr('스타일에 대한 연결 끊기'), !sel, breakLink)}
          {kind === 'cell' && menuItem(tr('스타일에 정의되지 않은 특성 지우기'), !sel, clearNotDefined)}
          {menuItem(tr('스타일 그룹으로 옮기기…'), !ownable, () => setSub('move'))}
          {menuItem(tr('그룹 이름 바꾸기…'), !group, () => setSub('renameGroup'))}
          {menuItem(tr('그룹 삭제'), !group || (kind === 'table' && !emptyGroup(group, groups, list)), deleteGroup)}
          {menuItem(tr('모든 스타일 그룹 열기'), false, () => setClosed(new Set()))}
          {menuItem(tr('모든 스타일 그룹 닫기'), false, () => setClosed(new Set(groups.map((g) => g.id))))}
          <button role="menuitemcheckbox" aria-checked={small} onClick={() => { setMenu(false); setSmall((v) => !v); }}>{(small ? '✓ ' : '') + tr('작은 패널 행')}</button>
        </div>
      )}
      {sub === 'newGroup' && <NameDialog title={tr('새 스타일 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={(n) => groups.some((g) => g.path.join('/') === [...(group?.path ?? []), n].join('/'))} onOk={newGroup} close={() => setSub(null)} />}
      {sub === 'renameGroup' && group && <NameDialog title={tr('그룹 이름 바꾸기')} label={tr('이름:')} initial={groupName(group)} taken={(n) => n !== groupName(group) && groups.some((g) => g.path.join('/') === [...group.path.slice(0, -1), n].join('/'))} onOk={renameGroup} close={() => setSub(null)} />}
      {sub === 'duplicate' && current && <NameDialog title={tr('스타일 복제')} label={tr('이름:')} initial={`${current.name} copy`} taken={(n) => sameName(current.group ?? [], n)} onOk={duplicate} close={() => setSub(null)} />}
      {sub === 'move' && current && <MoveDialog groups={groups} current={current.group ?? []} onOk={move} close={() => setSub(null)} />}
      {sub === 'selection' && sel && <NameDialog title={tr('선택에서 새 스타일')} label={tr('이름:')} initial={kind === 'cell' ? tr('새 셀 스타일') : tr('새 표 스타일')} taken={(n) => sameName(group?.path ?? [], n)} onOk={(name) => {
        ctx.edit(tr('선택에서 새 스타일'), [{ op: 'createTableStyleFromSelection', kind, table: sel.table, cell: sel.from, name, group: group?.id ?? '' }]);
        setPicked(styleId(group?.path ?? [], name));
      }} close={() => setSub(null)} />}
      {load && <LoadStylesDialog ctx={ctx} kinds={load === 'both' ? 'tableCell' : kind} close={() => setLoad(false)} />}
      {!sel && <p className="p-note">{kind === 'table' ? tr('표의 셀을 선택하면 스타일을 적용할 수 있습니다.') : tr('셀을 선택하면 스타일을 적용할 수 있습니다.')}</p>}
      {sub === 'copyTo' && current && <GroupPickDialog title={tr('그룹에 복사')} groups={groups} close={() => setSub(null)} onOk={copyToGroup} />}
      {sub === 'groupFromStyles' && current && <NameDialog title={tr('스타일에서 새 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={(n) => groups.some((g) => g.path.join('/') === n)} onOk={groupFromStyles} close={() => setSub(null)} />}
      {error && <p role="alert">{error}</p>}
      <StyleRowMenu menu={kind === 'cell' ? 'styleCell' : 'styleTable'} at={rowMenu.at} commands={ctx.commands} actions={tableStyleRowActions(kind, actions)} name={current?.name ?? ''} close={rowMenu.close} />
      {dialog && <StyleDialog key={dialog.mode + (dialog.style?.id ?? '')} ctx={ctx} kind={kind} mode={dialog.mode} style={dialog.style} together={dialog.together} group={group} choose={setPicked} close={() => setDialog(null)} />}
    </div>
  );
}

function StyleDialog({ ctx, kind, mode, style, together, group, choose, close }: {
  ctx: PanelCtx; kind: Kind; mode: Mode; style: TableStyleInfo | null; together?: string[]; group: StyleGroupInfo | null; choose: (id: string) => void; close: () => void;
}) {
  const list = listOf(ctx.info, kind);
  const cells = listOf(ctx.info, 'cell');
  const swatches = useMemo(() => (ctx.info?.swatches ?? []).filter((s) => s.model !== 'gradient'), [ctx.info]);
  const defaults = useMemo(() => (kind === 'table' ? (list.find((s) => s.id === NONE.table)?.settings ?? {}) : {}), [kind, list]);
  const initial = useMemo<Record<string, string>>(() => (style ? effective(list, style.id, defaults) : { ...defaults }), [style, list, defaults]);
  const [name, setName] = useState(mode === 'create' ? (kind === 'cell' ? tr('새 셀 스타일') : tr('새 표 스타일')) : (style?.name ?? ''));
  const [basedOn, setBasedOn] = useState(style?.basedOn ?? '');
  const [values, setValues] = useState<Record<string, string>>(() => ({ ...initial }));
  const [replacement, setReplacement] = useState(NONE[kind]);
  const [tab, setTab] = useState<'general' | 'cells' | PatternTab>('general');
  const [error, setError] = useState('');
  const set = (key: string, value: string) => setValues((v) => ({ ...v, [key]: value }));
  const basedChoices = list.filter((s) => s.id !== style?.id && !s.id.includes('/$ID/'));
  const nameTaken = list.some((s) => s.name === name.trim() && s.id !== style?.id);
  const num = (key: string) => values[key] ?? '';
  const numOk = (keys: string[]) => keys.every((k) => (values[k] ?? '') === '' || Number.isFinite(Number(values[k])));
  const tabs: PatternTab[] = ['rowStrokes', 'columnStrokes', 'fills'];
  const patternsOk = kind === 'cell' || tabs.every((p) => patternsValid(p, values as PatternValues));
  const numericKeys = kind === 'cell'
    ? [...SIDES.map((s) => `${s}Inset`), ...SIDES.map((s) => `${s}EdgeStrokeWeight`), 'FillTint']
    : ['SpaceBefore', 'SpaceAfter', ...SIDES.map((s) => `${s}BorderStrokeWeight`)];
  const ok = (mode === 'delete' ? !!replacement : name.trim() !== '' && !nameTaken && numOk(numericKeys) && patternsOk);

  const submit = () => {
    setError('');
    if (mode === 'delete') {
      if (!style) return;
      ctx.edit(tr('스타일 삭제'), (together ?? [style.id]).map((id) => ({ op: 'deleteTableStyle' as const, kind, style: id, replacement })));
      choose(replacement);
      close();
      return;
    }
    // what changed: only the settings the person touched; one emptied is taken back
    const props: Record<string, string> = {};
    const plainKeys = kind === 'cell' ? Object.keys(values) : ['SpaceBefore', 'SpaceAfter', ...SIDES.flatMap((x) => [`${x}BorderStrokeWeight`, `${x}BorderStrokeColor`]),
      ...(['Header', 'Footer', 'LeftColumn', 'RightColumn', 'Body'] as const).map((r) => `${r}RegionCellStyle`)];
    for (const key of plainKeys) if ((values[key] ?? '') !== (initial[key] ?? '')) props[key] = values[key] ?? '';
    if (kind === 'table') Object.assign(props, ...tabs.map((p) => patternChanges(p, initial, values as PatternValues)));
    if (mode === 'create') {
      if (basedOn) props.BasedOn = basedOn;
      ctx.edit(tr('스타일 만들기'), [{ op: 'createTableStyle', kind, name: name.trim(), props, ...(group ? { group: group.id } : {}) }]);
      choose(`${kind === 'cell' ? 'CellStyle' : 'TableStyle'}/${[...(group?.path ?? []), name.trim()].join('%3a')}`);
    } else if (style) {
      if (basedOn !== (style.basedOn ?? '')) props.BasedOn = basedOn;
      const rename = name.trim() !== style.name ? name.trim() : undefined;
      if (rename || Object.keys(props).length) ctx.edit(tr('스타일 옵션'), [{ op: 'editTableStyle', kind, style: style.id, ...(rename ? { rename } : {}), props }]);
      if (rename) choose(`${kind === 'cell' ? 'CellStyle' : 'TableStyle'}/${[...(style.group ?? []), rename].join('%3a')}`);
    }
    close();
  };

  const swatchSelect = (key: string, label: string) => (
    <label key={key}>{label}
      <select aria-label={label} value={values[key] ?? ''} onChange={(e) => set(key, e.target.value)}>
        <option value="">{tr('(상속)')}</option>
        {swatches.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select></label>
  );
  const numberField = (key: string, label: string) => (
    <label key={key}>{label}<TextNumberField width={220} label={label} text={num(key)} onText={(t) => set(key, t)} format={formatNumeric} parse={parseNumeric} step={1}
      word={{ text: '', words: [''], active: num(key) === '', onChoose: () => set(key, '') }} /></label>
  );
  const names = sideName();
  const regions = regionName();

  return createPortal(
    <div className="style-modal" role="dialog" aria-label={mode === 'delete' ? tr('스타일 삭제') : tr('스타일 옵션')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className={`card dialog style-options table-options${kind === 'table' && tab !== 'general' && tab !== 'cells' ? ' pattern-options' : ''}`} onSubmit={(e) => { e.preventDefault(); if (ok) submit(); }}>
        <h2>{mode === 'delete' ? tr('스타일 삭제') : kind === 'cell' ? tr('셀 스타일 옵션') : tr('표 스타일 옵션')}</h2>
        {mode === 'delete' ? (
          <label>{tr('바꿀 스타일')}
            <select aria-label={tr('바꿀 스타일')} value={replacement} onChange={(e) => setReplacement(e.target.value)}>
              <option value={NONE[kind]}>{kind === 'cell' ? '[None]' : '[No table style]'}</option>
              {list.filter((s) => s.id !== style?.id && !(together ?? []).includes(s.id) && !s.id.includes('/$ID/')).map((s) => <option key={s.id} value={s.id}>{pathName(s)}</option>)}
            </select></label>
        ) : (
          <>
            {kind === 'table' && (
              <div className="tabs" role="tablist">
                {([['general', tr('일반')], ['cells', tr('셀 스타일')], ['rowStrokes', tr('행 획')], ['columnStrokes', tr('열 획')], ['fills', tr('칠')]] as [typeof tab, string][]).map(([k, label]) => (
                  <button key={k} type="button" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}>{label}</button>))}
              </div>
            )}
            {tab === 'general' && (
              <>
                <label>{tr('이름:')}<input aria-label={tr('스타일 이름')} autoFocus value={name} onChange={(e) => setName(e.target.value)} /></label>
                {nameTaken && <p className="field-error" role="alert">{tr('같은 이름의 스타일이 이미 있습니다.')}</p>}
                <label>{tr('기준:')}
                  <select aria-label={tr('기준 스타일')} value={basedOn} onChange={(e) => setBasedOn(e.target.value)}>
                    <option value="">{kind === 'cell' ? '[None]' : '[No table style]'}</option>
                    {basedChoices.map((s) => <option key={s.id} value={s.id}>{pathName(s)}</option>)}
                  </select></label>
                {kind === 'cell' && (
                  <>
                    <label>{tr('단락 스타일:')}
                      <select aria-label={tr('단락 스타일')} value={values.AppliedParagraphStyle ?? NO_PARAGRAPH} onChange={(e) => set('AppliedParagraphStyle', e.target.value)}>
                        <option value={NO_PARAGRAPH}>{tr('[단락 스타일 없음]')}</option>
                        {(ctx.info?.paragraphStyles ?? []).filter((p) => !p.id.includes('/$ID/')).map((p) => <option key={p.id} value={p.id}>{[...p.group, p.name].join(' / ')}</option>)}
                      </select></label>
                    <label>{tr('세로 정렬:')}
                      <select aria-label={tr('세로 정렬')} value={values.VerticalJustification ?? ''} onChange={(e) => set('VerticalJustification', e.target.value)}>
                        <option value="">{tr('(상속)')}</option>
                        {([['TopAlign', tr('위쪽')], ['CenterAlign', tr('가운데')], ['BottomAlign', tr('아래쪽')], ['JustifyAlign', tr('양쪽 정렬')]] as const).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                      </select></label>
                    <fieldset><legend>{tr('칠')}</legend>{swatchSelect('FillColor', tr('칠 색상:'))}{numberField('FillTint', tr('색조(%):'))}</fieldset>
                    <fieldset><legend>{tr('안쪽 여백')}</legend>{SIDES.map((s) => numberField(`${s}Inset`, `${names[s]}:`))}</fieldset>
                    <fieldset><legend>{tr('셀 획')}</legend>
                      {SIDES.map((s) => <div key={s} className="style-edge">{numberField(`${s}EdgeStrokeWeight`, `${names[s]} ${tr('두께:')}`)}{swatchSelect(`${s}EdgeStrokeColor`, `${names[s]} ${tr('색상:')}`)}</div>)}
                    </fieldset>
                  </>
                )}
                {kind === 'table' && (
                  <>
                    <fieldset><legend>{tr('표 간격')}</legend>{numberField('SpaceBefore', tr('위 간격:'))}{numberField('SpaceAfter', tr('아래 간격:'))}</fieldset>
                    <fieldset><legend>{tr('표 테두리')}</legend>
                      {SIDES.map((s) => <div key={s} className="style-edge">{numberField(`${s}BorderStrokeWeight`, `${names[s]} ${tr('두께:')}`)}{swatchSelect(`${s}BorderStrokeColor`, `${names[s]} ${tr('색상:')}`)}</div>)}
                    </fieldset>
                  </>
                )}
              </>
            )}
            {kind === 'table' && tab === 'cells' && (
              <fieldset><legend>{tr('영역별 셀 스타일')}</legend>
                {(['Header', 'Footer', 'LeftColumn', 'RightColumn', 'Body'] as const).map((r) => (
                  <label key={r}>{regions[r]}:
                    <select aria-label={regions[r]} value={values[`${r}RegionCellStyle`] ?? ''} onChange={(e) => set(`${r}RegionCellStyle`, e.target.value)}>
                      <option value="">{tr('(없음)')}</option>
                      {cells.filter((c) => !c.id.includes('/$ID/')).map((c) => <option key={c.id} value={c.id}>{pathName(c)}</option>)}
                    </select></label>))}
              </fieldset>
            )}
            {kind === 'table' && tab === 'rowStrokes' && <StrokePatterns dim="Row" values={values} set={set} swatches={swatches} />}
            {kind === 'table' && tab === 'columnStrokes' && <StrokePatterns dim="Column" values={values} set={set} swatches={swatches} />}
            {kind === 'table' && tab === 'fills' && <FillPatterns values={values} set={set} swatches={swatches} />}
          </>
        )}
        {error && <p role="alert">{error}</p>}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!ok || !ctx.editable}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

