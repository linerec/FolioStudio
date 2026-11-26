import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../../../i18n';
import { fieldKey } from '../../../ui/keys';
import { DbError, computedValues, dbColumns, dbPreview, dbTables, type ColumnInfo, type Connection, type ConnectionSettings, type Preview, type Query, type TableInfo, type Refresh } from './api';
import { FilterBuilder, SortEditor } from './FilterBuilder';
import { AskParams, ParamsEditor } from './Params';
import { REFRESH_MODES, cleanQuery, driverName, emptyFilter, explain, fromHint, newQuery, refreshLabel, runnable } from './model';
import './db.css';

type Load<T> = { state: 'idle' | 'loading' | 'ready'; data: T } | { state: 'failed'; data: T; kind: string; detail: string };
const failure = (e: unknown, data: never[] = []): Load<never[]> => e instanceof DbError ? { state: 'failed', data, kind: e.kind, detail: e.message } : { state: 'failed', data, kind: 'unknown', detail: String(e) };

/** An error of a database call, as a sentence for the person (what it is, then the database's own words). */
export function ErrorLine({ kind, detail, retry, unlock }: { kind: string; detail: string; retry?: () => void; unlock?: () => void }) {
  return <div role="alert" className="db-error" data-db-error-kind={kind}>
    <p>{explain(kind)}</p>
    <small className="db-detail">{detail}</small>
    {kind === 'needsPassword' && unlock ? <button type="button" data-db-unlock onClick={unlock}>{tr('비밀번호 입력…')}</button>
      : retry && <button type="button" onClick={retry}>{tr('다시 시도')}</button>}
  </div>;
}

type Shown = { preview: Preview; computed: string[][]; computedNames: string[] };
type PreviewState = { state: 'idle' } | { state: 'loading'; last?: Shown } | { state: 'ready'; shown: Shown } | { state: 'failed'; kind: string; detail: string; last?: Shown };

/** The first rows the query gives, the number of rows in all, and the SQL that ran (the values are shown apart: they are bound, never put into the SQL). */
function PreviewTable({ state, unlock }: { state: PreviewState; unlock: () => void }) {
  const shown = state.state === 'ready' ? state.shown : state.state === 'idle' ? undefined : state.last;
  if (state.state === 'idle') return <p className="db-hint" data-db-preview="idle">{tr('테이블을 고르면 여기에 처음 50행이 나타납니다.')}</p>;
  const p = shown?.preview;
  return <div className="db-preview" data-db-preview={state.state} aria-busy={state.state === 'loading'}>
    {state.state === 'failed' && <ErrorLine kind={state.kind} detail={state.detail} unlock={unlock} />}
    {p && <>
      <div className="db-preview-head" data-db-preview-summary data-db-rows={p.count ?? ''} data-db-shown={p.records.length}>
        {state.state === 'loading' ? tr('불러오는 중…') : p.count !== null && p.count !== undefined
          ? tr('전체 {0}행 중 처음 {1}행 · {2}ms', { 0: p.count, 1: p.records.length, 2: p.ms })
          : tr('처음 {0}행 · {1}ms', { 0: p.records.length, 1: p.ms })}
      </div>
      <div className="db-table-scroll">
        <table data-db-preview-table>
          <thead><tr>
            {p.columns.map((c) => <th key={c}>{c}</th>)}
            {shown!.computedNames.map((c) => <th key={`ƒ${c}`} className="db-computed" title={tr('계산 열')}>ƒ {c}</th>)}
          </tr></thead>
          <tbody>
            {p.records.map((row, i) => <tr key={i}>
              {row.map((v, j) => <td key={j} className={v === null ? 'db-null' : undefined}>{v === null ? 'NULL' : v}</td>)}
              {shown!.computed[i]?.map((v, j) => <td key={`ƒ${j}`} className="db-computed" data-db-computed-value>{v}</td>)}
            </tr>)}
          </tbody>
        </table>
      </div>
      <details className="db-sql" data-db-sql>
        <summary>{tr('쓰인 SQL 보기')}</summary>
        <pre data-db-sql-text>{p.sql}</pre>
        {p.params.length > 0 && <p className="db-binds" data-db-binds>{tr('묶인 값: {0}', { 0: p.params.map((v) => JSON.stringify(v)).join(', ') })}</p>}
        <small>{tr('값은 SQL 문장에 끼워 넣지 않고 따로 묶어서 보냅니다.')}</small>
      </details>
    </>}
  </div>;
}

type Tab = 'table' | 'filter' | 'sql' | 'params';

/**
 * Data Merge ▸ Connect to Database: the connection, the table and its columns, the conditions and the sort, the parameters, the advanced SQL tab,
 * and a preview of what comes back. The dialog only edits the query definition (`Query`, kept in the document) and shows what the engine says
 * (`dbTables`, `dbColumns`, `dbPreview`): the SQL is made there. Import is the engine command `dataMergeSelectDatabase` (`onImport`).
 */
export function QueryDialog({ initial, connections, sqliteFiles, askPassword, computed, lastValues, editable, title, openConnections, onImport, close }: {
  initial: Query;
  connections: Connection[];
  sqliteFiles: string[];
  /** Ask for a connection's password (the engine said `needsPassword`); resolves `true` once the engine kept it. */
  askPassword: (connection: string) => Promise<boolean>;
  /** The computed fields (mergeexpr-c1) of the document: their values are shown beside the columns. */
  computed: { name: string; expr: string }[];
  /** The values the parameters had when the data was fetched last. */
  lastValues: Record<string, unknown>;
  editable: boolean;
  title: string;
  openConnections: (fill?: ConnectionSettings, select?: string) => void;
  onImport: (query: Query, values?: Record<string, unknown>) => Promise<{ ok: true } | { ok: false; error: string }>;
  close: () => void;
}) {
  const [q, setQ] = useState<Query>(initial);
  const [advanced, setAdvanced] = useState(!!initial.sql);
  const [tab, setTab] = useState<Tab>('table');
  const [tables, setTables] = useState<Load<TableInfo[]>>({ state: 'idle', data: [] });
  const [cols, setCols] = useState<Load<ColumnInfo[]>>({ state: 'idle', data: [] });
  const [search, setSearch] = useState('');
  const [preview, setPreview] = useState<PreviewState>({ state: 'idle' });
  const [asking, setAsking] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState('');
  const serial = useRef({ tables: 0, cols: 0, preview: 0 });
  // the password question: asked by itself the first time a connection says `needsPassword` in this dialog, then by the button next to the message;
  // an answer makes the lists and the preview ask the engine again
  const [unlockedCount, setUnlockedCount] = useState(0);
  const asked = useRef(new Set<string>());

  const connection = connections.find((c) => c.name === q.connection);
  const columns = cols.data;
  const clean = useMemo(() => cleanQuery(q, columns, advanced), [q, columns, advanced]);
  const cleanKey = JSON.stringify(clean) + (computed.length ? JSON.stringify(computed) : '');
  const unlock = async (name: string) => { if (await askPassword(name)) setUnlockedCount((n) => n + 1); };
  const unlockOnce = (name: string) => { if (!asked.current.has(name)) { asked.current.add(name); void unlock(name); } };
  const patch = (p: Partial<Query>) => setQ((o) => ({ ...o, ...p }));

  // the tables of the connection
  const loadTables = async () => {
    const mine = ++serial.current.tables;
    if (!connection) { setTables({ state: 'idle', data: [] }); return; }
    setTables({ state: 'loading', data: [] });
    try {
      const r = await dbTables(connection.name);
      if (serial.current.tables === mine) setTables({ state: 'ready', data: r.tables });
    } catch (e) {
      if (serial.current.tables !== mine) return;
      setTables(failure(e));
      if (e instanceof DbError && e.kind === 'needsPassword') unlockOnce(connection.name);
    }
  };
  useEffect(() => { void loadTables(); }, [connection?.name, connection?.database, connection?.host, connection?.hasPassword, unlockedCount]);

  // the columns of the table
  const tableKey = q.table ? `${q.table.schema ?? ''}/${q.table.name}` : '';
  useEffect(() => {
    const mine = ++serial.current.cols;
    if (!connection || !q.table || advanced && !q.table) { setCols({ state: 'idle', data: [] }); return; }
    setCols({ state: 'loading', data: [] });
    dbColumns(connection.name, q.table).then(
      (r) => { if (serial.current.cols === mine) setCols({ state: 'ready', data: r.columns }); },
      (e) => { if (serial.current.cols === mine) setCols(failure(e)); });
  }, [connection?.name, tableKey, unlockedCount]);

  // the preview follows the query a moment after the last change; an answer that is no longer the newest question is dropped
  useEffect(() => {
    const mine = ++serial.current.preview;
    if (!connection || !runnable(clean, advanced) || (!advanced && cols.state !== 'ready')) { setPreview({ state: 'idle' }); return; }
    setPreview((p) => p.state === 'ready' ? { state: 'loading', last: p.shown } : p.state === 'loading' || p.state === 'failed' ? { ...p, state: 'loading' } as PreviewState : { state: 'loading' });
    const timer = window.setTimeout(async () => {
      try {
        const r = await dbPreview(clean, undefined, 50);
        let values: string[][] = [];
        const names = computed.map((c) => c.name);
        if (computed.length) {
          const records = r.records.map((row) => Object.fromEntries(r.fields.map((f, i) => [f.name, row[i] ?? ''])));
          values = await Promise.all(records.map((rec) => computedValues(computed, rec).catch(() => computed.map(() => ''))));
        }
        if (serial.current.preview === mine) setPreview({ state: 'ready', shown: { preview: r, computed: values, computedNames: names } });
      } catch (e) {
        if (serial.current.preview !== mine) return;
        const f = e instanceof DbError ? { kind: e.kind, detail: e.message } : { kind: 'unknown', detail: String(e) };
        setPreview((p) => ({ state: 'failed', ...f, last: p.state === 'ready' ? p.shown : p.state === 'loading' ? p.last : undefined }));
        if (f.kind === 'needsPassword' && connection) unlockOnce(connection.name);
      }
    }, 350);
    return () => window.clearTimeout(timer);
  }, [cleanKey, cols.state, connection?.name, advanced, unlockedCount]);

  const chooseConnection = (name: string) => {
    const c = connections.find((x) => x.name === name);
    setQ(newQuery(name, c ? { driver: c.driver, host: c.host, port: c.port, database: c.database } : undefined));
    setAdvanced(false);
    setPreview({ state: 'idle' });
  };
  const chooseTable = (t: TableInfo) => {
    if (q.table?.name === t.name && (q.table.schema ?? null) === (t.schema ?? null)) return;
    setQ((o) => ({ ...o, table: { schema: t.schema, name: t.name }, columns: [], filter: undefined, orderBy: [] }));
  };
  const selectedColumns = q.columns.length ? new Set(q.columns) : new Set(columns.map((c) => c.name));
  const toggleColumn = (name: string, on: boolean) => {
    const next = new Set(selectedColumns);
    if (on) next.add(name); else next.delete(name);
    if (!next.size) return;
    patch({ columns: next.size === columns.length ? [] : columns.filter((c) => next.has(c.name)).map((c) => c.name) });
  };
  const usedParam = (name: string) => JSON.stringify(q.filter ?? {}).includes(`"param":${JSON.stringify(name)}`) || (q.sql ?? '').includes(`:${name}`);

  const visibleTables = tables.data.filter((t) => t.name.toLowerCase().includes(search.trim().toLowerCase()) || (t.schema ?? '').toLowerCase().includes(search.trim().toLowerCase()));
  const schemas = [...new Set(visibleTables.map((t) => t.schema ?? ''))];
  const driver = connection?.driver ?? 'postgres';
  const ready = !!connection && runnable(clean, advanced) && editable && !importing;

  const doImport = async (values?: Record<string, unknown>) => {
    setImporting(true); setImportError('');
    const r = await onImport(clean, values);
    setImporting(false);
    if (r.ok) close(); else setImportError(r.error);
  };

  const numberOrNone = (v: string): number | undefined => { const n = parseInt(v, 10); return Number.isFinite(n) && n > 0 ? n : undefined; };
  const filter = q.filter ?? emptyFilter();

  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} data-db-query-dialog onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !importing && !asking) close(); }}>
      <div className="card dialog db-dialog db-query">
        <h2>{title}</h2>
        <div className="db-connection-bar">
          <label>{tr('연결')}
            <select aria-label={tr('연결')} data-db-connection-select value={connection ? q.connection : ''} onChange={(e) => chooseConnection(e.target.value)}>
              {!connection && <option value="">{q.connection ? tr('{0} (이 컴퓨터에 없음)', { 0: q.connection }) : tr('(연결 고르기)')}</option>}
              {connections.map((c) => <option key={c.name} value={c.name}>{c.name} · {driverName(c.driver)}</option>)}
            </select>
          </label>
          <button type="button" data-db-open-connections onClick={() => openConnections(undefined, q.connection)}>{tr('연결 설정…')}</button>
          {!connection && q.connection && <button type="button" data-db-fill-connection onClick={() => openConnections(fromHint(q.connection, q.hint))}>{tr('이 이름으로 연결 만들기')}</button>}
        </div>
        {!connection && q.connection && <p role="status" className="db-warn" data-db-connection-missing>
          {tr('연결 "{0}" 이(가) 이 컴퓨터에 없습니다. 문서에 마지막으로 가져온 값이 남아 있어 인쇄와 내보내기는 그대로 됩니다. 새로 고치려면 같은 이름으로 연결을 만드세요.', { 0: q.connection })}
          {q.hint && <> {tr('문서에 남은 힌트: {0} {1} {2}', { 0: driverName(q.hint.driver), 1: q.hint.host, 2: q.hint.database })}</>}
        </p>}
        {!editable && <p role="status" className="db-warn">{tr('이 문서는 읽기 전용이라 가져올 수 없습니다. 미리 보기만 할 수 있습니다.')}</p>}
        <div className="db-tabs" role="tablist">
          {([['table', tr('테이블·열')], ['filter', tr('조건·정렬')], ['sql', tr('고급 SQL')], ['params', tr('매개변수')]] as [Tab, string][]).map(([id, label]) =>
            <button key={id} type="button" role="tab" aria-selected={tab === id} data-db-tab={id} onClick={() => setTab(id)}>{label}{id === 'params' && q.params.length ? ` (${q.params.length})` : ''}</button>)}
        </div>
        <div className="db-tabpanel" role="tabpanel" data-db-panel={tab}>
          {tab === 'table' && <div className="db-tables-columns">
            <div className="db-tablelist" data-db-tables>
              <input type="search" aria-label={tr('테이블 검색')} data-db-table-search placeholder={tr('테이블 검색')} value={search} onChange={(e) => setSearch(e.target.value)} />
              {tables.state === 'loading' && <p role="status">{tr('테이블 목록을 읽는 중…')}</p>}
              {tables.state === 'failed' && <ErrorLine kind={tables.kind} detail={tables.detail} retry={() => void loadTables()} unlock={connection ? () => void unlock(connection.name) : undefined} />}
              {!connection && <p className="db-hint">{tr('먼저 연결을 고르세요.')}</p>}
              <ul>
                {schemas.map((s) => <li key={s} className="db-schema">
                  {(s !== '' || schemas.length > 1) && <span className="db-schema-name">{s || tr('(스키마 없음)')}</span>}
                  <ul>
                    {visibleTables.filter((t) => (t.schema ?? '') === s).map((t) => <li key={t.name} data-db-table={t.name} data-db-kind={t.kind} aria-selected={q.table?.name === t.name && (q.table?.schema ?? '') === s}>
                      <button type="button" onClick={() => chooseTable(t)}>
                        <span>{t.name}</span><small className="db-kind">{t.kind === 'view' ? tr('뷰') : tr('테이블')}</small>
                      </button>
                    </li>)}
                  </ul>
                </li>)}
              </ul>
              {tables.state === 'ready' && !visibleTables.length && <p className="db-hint">{tables.data.length ? tr('검색과 맞는 테이블이 없습니다.') : tr('이 연결에는 읽을 수 있는 테이블이 없습니다.')}</p>}
            </div>
            <div className="db-columnlist" data-db-columns>
              {q.table ? <>
                <div className="db-columnlist-head">
                  <b>{q.table.schema ? `${q.table.schema}.` : ''}{q.table.name}</b>
                  <span>
                    <button type="button" onClick={() => patch({ columns: [] })} disabled={!q.columns.length}>{tr('모두 선택')}</button>
                  </span>
                </div>
                {cols.state === 'loading' && <p role="status">{tr('열 목록을 읽는 중…')}</p>}
                {cols.state === 'failed' && <ErrorLine kind={cols.kind} detail={cols.detail} unlock={connection ? () => void unlock(connection.name) : undefined} />}
                <table className="db-columns-table">
                  <thead><tr><th>{tr('가져오기')}</th><th>{tr('열')}</th><th>{tr('형식')}</th></tr></thead>
                  <tbody>{columns.map((c) => <tr key={c.name} data-db-column-row={c.name}>
                    <td><input type="checkbox" aria-label={tr('{0} 열 가져오기', { 0: c.name })} checked={selectedColumns.has(c.name)} disabled={selectedColumns.size === 1 && selectedColumns.has(c.name)} onChange={(e) => toggleColumn(c.name, e.target.checked)} /></td>
                    <td>{c.name}</td><td className="db-type">{c.type || '?'}{c.nullable ? '' : ' · NOT NULL'}</td>
                  </tr>)}</tbody>
                </table>
              </> : <p className="db-hint">{tr('왼쪽에서 테이블이나 뷰를 고르세요.')}</p>}
            </div>
          </div>}
          {tab === 'filter' && <div className="db-filter-sort">
            {advanced && <p role="status" className="db-warn">{tr('고급 SQL 을 쓰고 있어 테이블·조건·정렬은 쓰이지 않습니다.')}</p>}
            <h3>{tr('조건')}</h3>
            <FilterBuilder filter={filter} columns={columns} driver={driver} params={q.params} onChange={(g) => patch({ filter: g })} />
            <h3>{tr('정렬')}</h3>
            <SortEditor orderBy={q.orderBy} columns={columns} onChange={(o) => patch({ orderBy: o })} />
            <h3>{tr('개수 제한')}</h3>
            <div className="db-limits">
              <label>{tr('가져올 최대 행 수')}<input aria-label={tr('가져올 최대 행 수')} data-db-limit inputMode="numeric" placeholder={tr('전부')} value={q.limit ?? ''} onChange={(e) => patch({ limit: numberOrNone(e.target.value) })} /></label>
              <label>{tr('건너뛸 행 수')}<input aria-label={tr('건너뛸 행 수')} data-db-offset inputMode="numeric" placeholder="0" value={q.offset ?? ''} onChange={(e) => patch({ offset: numberOrNone(e.target.value) })} /></label>
            </div>
            <small>{tr('건너뛸 행 수를 쓰려면 정렬을 정해 두세요. 한 번에 가져올 수 있는 행은 최대 50,000개입니다.')}</small>
          </div>}
          {tab === 'sql' && <div className="db-advanced">
            <label className="check"><input type="checkbox" data-db-advanced checked={advanced} onChange={(e) => setAdvanced(e.target.checked)} />{tr('고급 SQL 사용 (테이블·조건·정렬 대신)')}</label>
            <p className="db-hint">{tr('SELECT 나 WITH 로 시작하는 문장 하나만 쓸 수 있습니다. 읽기 전용으로 실행되어 데이터를 바꾸지 못합니다. 매개변수는 :이름 으로 씁니다. 예: WHERE issue_date >= :시작')}</p>
            <textarea aria-label={tr('SQL')} data-db-sql-input rows={9} spellCheck={false} disabled={!advanced} placeholder="SELECT id, title FROM ads WHERE status = :status ORDER BY id" value={q.sql ?? ''} onChange={(e) => patch({ sql: e.target.value })} />
          </div>}
          {tab === 'params' && <ParamsEditor params={q.params} used={usedParam} onChange={(params) => patch({ params })} />}
        </div>
        <PreviewTable state={preview} unlock={() => { if (connection) void unlock(connection.name); }} />
        {importError && <p role="alert" data-db-import-error>{importError}</p>}
        <div className="dialog-actions">
          <label className="db-refresh">{tr('새로 고침 방식')}
            <select aria-label={tr('새로 고침 방식')} data-db-refresh value={q.refresh} onChange={(e) => patch({ refresh: e.target.value as Refresh })}>
              {REFRESH_MODES.map((m) => <option key={m} value={m}>{refreshLabel(m)}</option>)}
            </select>
          </label>
          <span className="db-spacer" />
          <button type="button" data-db-cancel disabled={importing} onClick={close}>{tr('취소')}</button>
          <button type="button" data-db-import disabled={!ready} onClick={() => { if (clean.params.length) setAsking(true); else void doImport(); }}>{importing ? tr('가져오는 중…') : tr('가져오기')}</button>
        </div>
        {asking && <AskParams params={clean.params} initial={lastValues} title={tr('매개변수 값')} ok={tr('가져오기')} busy={importing} error={importError}
          onCancel={() => setAsking(false)} onOk={(v) => void doImport(v).then(() => setAsking(false))} />}
      </div>
    </div>,
    document.body,
  );
}
