import { tr } from '../../../i18n';
import type { ColumnInfo, Condition, Driver, FilterNode, Group, Order, Param, ParamType } from './api';
import { SHAPE, groupItems, groupKind, isGroup, makeGroup, newCondition, operatorLabel, operatorsFor, type OperatorId } from './model';

type Cell = 'text' | 'month' | 'date';
const asParam = (v: unknown): string | null => v && typeof v === 'object' && !Array.isArray(v) && typeof (v as { param?: unknown }).param === 'string' ? (v as { param: string }).param : null;
const PARAM_FOR: Record<Cell | 'list', ParamType[]> = { text: ['text', 'number', 'date', 'month'], month: ['month'], date: ['date'], list: ['list'] };

/** One value of a condition: typed in, or the value of a parameter of the query (`{param: name}`), which is asked for when the data is fetched. */
function ValueCell({ kind, value, params, label, onChange }: { kind: Cell; value: unknown; params: Param[]; label: string; onChange: (v: unknown) => void }) {
  const bound = asParam(value);
  const usable = params.filter((p) => PARAM_FOR[kind].includes(p.type));
  return <span className="db-value">
    {usable.length > 0 && <select aria-label={tr('{0}: 값 종류', { 0: label })} data-db-source={bound ? 'param' : 'value'} value={bound ?? ''} onChange={(e) => onChange(e.target.value ? { param: e.target.value } : '')}>
      <option value="">{tr('값')}</option>
      {usable.map((p) => <option key={p.name} value={p.name}>{tr('매개변수 {0}', { 0: p.name })}</option>)}
    </select>}
    {!bound && <input aria-label={label} data-db-value type={kind === 'text' ? 'text' : kind} value={typeof value === 'string' || typeof value === 'number' ? String(value) : ''} onChange={(e) => onChange(e.target.value)} />}
  </span>;
}

function ListCell({ value, params, onChange }: { value: unknown; params: Param[]; onChange: (v: unknown) => void }) {
  const bound = asParam(value);
  const usable = params.filter((p) => PARAM_FOR.list.includes(p.type));
  const lines = Array.isArray(value) ? value.map(String).join('\n') : '';
  return <span className="db-value">
    {usable.length > 0 && <select aria-label={tr('목록: 값 종류')} data-db-source={bound ? 'param' : 'value'} value={bound ?? ''} onChange={(e) => onChange(e.target.value ? { param: e.target.value } : [])}>
      <option value="">{tr('값')}</option>
      {usable.map((p) => <option key={p.name} value={p.name}>{tr('매개변수 {0}', { 0: p.name })}</option>)}
    </select>}
    {!bound && <textarea aria-label={tr('목록 (한 줄에 하나)')} data-db-value rows={2} placeholder={tr('한 줄에 하나')} value={lines} onChange={(e) => onChange(e.target.value.split('\n'))} />}
  </span>;
}

function Values({ c, params, onChange }: { c: Condition; params: Param[]; onChange: (v: unknown) => void }) {
  const shape = SHAPE[c.op as OperatorId] ?? 'one';
  if (shape === 'none') return null;
  if (shape === 'list') return <ListCell value={c.value} params={params} onChange={onChange} />;
  if (shape === 'one') return <ValueCell kind="text" label={tr('값')} value={c.value} params={params} onChange={onChange} />;
  if (shape === 'month') return <ValueCell kind="month" label={tr('월')} value={c.value} params={params} onChange={onChange} />;
  const kind: Cell = shape === 'dates' ? 'date' : 'text';
  const pair = Array.isArray(c.value) ? c.value : ['', ''];
  return <span className="db-pair">
    <ValueCell kind={kind} label={tr('시작')} value={pair[0]} params={params} onChange={(v) => onChange([v, pair[1] ?? ''])} />
    <span aria-hidden="true">~</span>
    <ValueCell kind={kind} label={tr('끝')} value={pair[1]} params={params} onChange={(v) => onChange([pair[0] ?? '', v])} />
  </span>;
}

function ConditionRow({ c, columns, driver, params, change, remove }: { c: Condition; columns: ColumnInfo[]; driver: Driver; params: Param[]; change: (c: Condition) => void; remove: () => void }) {
  const column = columns.find((x) => x.name === c.column);
  const operators = operatorsFor(column?.type ?? '', driver);
  return <div className="db-condition" data-db-condition>
    <select aria-label={tr('조건의 열')} data-db-column value={c.column} onChange={(e) => {
      const next = columns.find((x) => x.name === e.target.value);
      const ops = operatorsFor(next?.type ?? '', driver);
      // the operator stays when the new column has it, else the first one that fits; the value is kept only for the same operator
      const op = ops.includes(c.op as OperatorId) ? c.op : ops[0];
      change({ column: e.target.value, op, value: op === c.op ? c.value : '' });
    }}>
      {!column && <option value={c.column}>{c.column || tr('(열 고르기)')}</option>}
      {columns.map((x) => <option key={x.name} value={x.name}>{x.name} ({x.type || '?'})</option>)}
    </select>
    <select aria-label={tr('조건의 연산자')} data-db-op value={c.op} onChange={(e) => {
      const was = SHAPE[c.op as OperatorId], now = SHAPE[e.target.value as OperatorId];
      change({ ...c, op: e.target.value, value: was === now ? c.value : now === 'list' ? [] : now === 'two' || now === 'dates' ? ['', ''] : '' });
    }}>
      {operators.map((o) => <option key={o} value={o}>{operatorLabel(o)}</option>)}
    </select>
    <Values c={c} params={params} onChange={(value) => change({ ...c, value })} />
    <button type="button" aria-label={tr('조건 삭제')} data-db-remove-condition onClick={remove}>×</button>
  </div>;
}

function GroupBox({ group, columns, driver, params, change, remove, depth }: { group: Group; columns: ColumnInfo[]; driver: Driver; params: Param[]; change: (g: Group) => void; remove?: () => void; depth: number }) {
  const items = groupItems(group), kind = groupKind(group);
  const setItems = (next: FilterNode[]) => change(makeGroup(kind, next));
  return <div className="db-group" data-db-group={kind} data-depth={depth}>
    <div className="db-group-head">
      <span>{depth === 0 ? tr('다음 조건을') : tr('이 묶음은')}</span>
      <select aria-label={tr('묶음 방식')} data-db-joiner value={kind} onChange={(e) => change(makeGroup(e.target.value as 'all' | 'any', items))}>
        <option value="all">{tr('모두 만족')}</option>
        <option value="any">{tr('하나라도 만족')}</option>
      </select>
      {remove && <button type="button" aria-label={tr('묶음 삭제')} data-db-remove-group onClick={remove}>×</button>}
    </div>
    {items.map((item, i) => isGroup(item)
      ? <GroupBox key={i} group={item} columns={columns} driver={driver} params={params} depth={depth + 1}
          change={(g) => setItems(items.map((x, j) => j === i ? g : x))} remove={() => setItems(items.filter((_, j) => j !== i))} />
      : <ConditionRow key={i} c={item} columns={columns} driver={driver} params={params}
          change={(c) => setItems(items.map((x, j) => j === i ? c : x))} remove={() => setItems(items.filter((_, j) => j !== i))} />)}
    <div className="db-group-actions">
      <button type="button" data-db-add-condition disabled={!columns.length} onClick={() => setItems([...items, newCondition(columns, driver)])}>{tr('조건 추가')}</button>
      {depth === 0 && <button type="button" data-db-add-group disabled={!columns.length} onClick={() => setItems([...items, makeGroup('any', [newCondition(columns, driver)])])}>{tr('묶음 추가')}</button>}
    </div>
  </div>;
}

/** The conditions of the query: rows of column, operator and value (typed or a parameter), grouped "all" or "any", two levels deep. */
export function FilterBuilder({ filter, columns, driver, params, onChange }: { filter: Group; columns: ColumnInfo[]; driver: Driver; params: Param[]; onChange: (g: Group) => void }) {
  return <div className="db-filter" data-db-filter>
    <GroupBox group={filter} columns={columns} driver={driver} params={params} change={onChange} depth={0} />
    {!groupItems(filter).length && <p className="db-hint">{tr('조건이 없으면 테이블의 모든 행을 가져옵니다.')}</p>}
  </div>;
}

/** The sort: several columns, each ascending or descending, in the order they are applied. */
export function SortEditor({ orderBy, columns, onChange }: { orderBy: Order[]; columns: ColumnInfo[]; onChange: (o: Order[]) => void }) {
  const move = (i: number, d: number) => { const next = [...orderBy]; const j = i + d; if (j < 0 || j >= next.length) return; [next[i], next[j]] = [next[j], next[i]]; onChange(next); };
  return <div className="db-sort" data-db-sort>
    {orderBy.map((o, i) => <div key={i} className="db-sort-row" data-db-sort-row>
      <span className="db-sort-no">{i + 1}</span>
      <select aria-label={tr('정렬 열')} data-db-sort-column value={o.column} onChange={(e) => onChange(orderBy.map((x, j) => j === i ? { ...x, column: e.target.value } : x))}>
        {columns.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
      </select>
      <select aria-label={tr('정렬 방향')} data-db-sort-dir value={o.dir} onChange={(e) => onChange(orderBy.map((x, j) => j === i ? { ...x, dir: e.target.value as 'asc' | 'desc' } : x))}>
        <option value="asc">{tr('오름차순')}</option>
        <option value="desc">{tr('내림차순')}</option>
      </select>
      <button type="button" aria-label={tr('위로')} disabled={i === 0} onClick={() => move(i, -1)}>↑</button>
      <button type="button" aria-label={tr('아래로')} disabled={i === orderBy.length - 1} onClick={() => move(i, 1)}>↓</button>
      <button type="button" aria-label={tr('정렬 삭제')} onClick={() => onChange(orderBy.filter((_, j) => j !== i))}>×</button>
    </div>)}
    <button type="button" data-db-add-sort disabled={!columns.length} onClick={() => onChange([...orderBy, { column: columns.find((c) => !orderBy.some((o) => o.column === c.name))?.name ?? columns[0].name, dir: 'asc' }])}>{tr('정렬 추가')}</button>
  </div>;
}

