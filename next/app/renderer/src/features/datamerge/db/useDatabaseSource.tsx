import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { tr } from '../../../i18n';
import type { EditCommand, EditResult, DocumentSummary } from '../../../engine';
import type { DocumentInfo } from '../../../inspect';
import type { TreeEntry } from '../../../project';
import { DbError, dbConnections, dbPreview, lockConnection, sourceStatus, type Connection, type ConnectionSettings, type Query, type Refresh, type SourceStatus } from './api';
import { ConnectionDialog } from './ConnectionDialog';
import { QueryDialog } from './QueryDialog';
import { AskParams } from './Params';
import { PasswordPrompt } from './PasswordPrompt';
import { explain, newQuery } from './model';

type DatabaseStatus = Extract<SourceStatus, { kind: 'database' }>;

/** What the Data Merge panel does with the database source (PanelCtx.databaseSource). */
export type DatabaseActions = {
  /** A project document is open (the database source is kept in it). */
  available: boolean;
  editable: boolean;
  /** The document's data source when it is a database: what the engine says (`dataMergeSourceStatus`). */
  status: DatabaseStatus | null;
  /** The saved connections of this computer. */
  connections: Connection[];
  /** The query is being run to see whether the data changed. */
  checking: boolean;
  /** The document's connection needs a password this session does not have (found when the document was opened or the source was used): the panel says so. */
  locked: boolean;
  /** Ask for the password of the document's connection (then do what the refresh mode says for an opened document). */
  unlock: () => void;
  /** Forget the password this session kept for the document's connection. */
  lock: () => void;
  /** Data Merge ▸ Connect to Database…: choose a connection, a table, conditions; import. */
  connect: () => void;
  /** The connections of this computer (list, new, change, test). */
  manageConnections: (fill?: ConnectionSettings) => void;
  /** Change the query the document remembers. */
  editQuery: () => void;
  /** Run the remembered query again (asks the parameters' values first when it has any). */
  refresh: () => void;
  /** Run the query and compare its result with what the document has (marks the status "changed"). */
  check: () => void;
  setRefreshMode: (mode: Refresh) => void;
};

type Context = {
  doc: DocumentSummary | null;
  info: DocumentInfo | null;
  editable: boolean;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
  flash: (message: string) => void;
  projectEntries: TreeEntry[];
};

const SQLITE = /\.(sqlite3?|db3?)$/i;
function filesOf(entries: TreeEntry[]): string[] {
  const out: string[] = [];
  const walk = (list: TreeEntry[]) => { for (const e of list) { if (e.kind === 'dir') walk(e.children); else if (SQLITE.test(e.name)) out.push(e.path); } };
  walk(entries);
  return out;
}

type Ask = { title: string; message?: string; ok: string; run: (values?: Record<string, unknown>) => Promise<string | null> };

/**
 * Data Merge ▸ a database as the data source. The screens (connections, the query, the question for the parameters' values) and what happens when a
 * document is opened (the document's refresh mode: manual, ask, automatic) live here; everything they do is an engine command or query
 * (`dbConnections`, `dbTables`, `dbPreview`, `dataMergeSelectDatabase`, `dataMergeRefresh`, `dataMergeSourceStatus`).
 */
export function useDatabaseSource(ctx: Context) {
  const { doc, info, editable, edit, flash } = ctx;
  const [connections, setConnections] = useState<Connection[]>([]);
  const [status, setStatus] = useState<DatabaseStatus | null>(null);
  const [checking, setChecking] = useState(false);
  // the connection whose password is missing (a use of the source answered `needsPassword`): the panel shows "password needed · unlock"
  const [locked, setLocked] = useState<string | null>(null);
  const [credentialStore, setCredentialStore] = useState(true);
  const [pw, setPw] = useState<{ connection: string; resolve: (unlocked: boolean) => void } | null>(null);
  const [query, setQuery] = useState<{ initial: Query; title: string } | null>(null);
  const [manage, setManage] = useState<{ fill?: ConnectionSettings; select?: string } | null>(null);
  const [ask, setAsk] = useState<(Ask & { error: string; busy: boolean; params: Query['params']; last: Record<string, unknown> }) | null>(null);
  const available = !!doc?.folio;
  const key = info?.dataMergeSource?.file ?? '';
  const editRef = useRef(ctx); editRef.current = ctx;
  const sqliteFiles = useMemo(() => filesOf(ctx.projectEntries), [ctx.projectEntries]);

  // the connections of this computer and the state of the document's source: read again when the source changes (import, refresh, undo, redo)
  const load = useCallback(async (check = false): Promise<DatabaseStatus | null> => {
    if (!doc?.folio) { setStatus(null); return null; }
    try {
      const s = await sourceStatus(check);
      const db = s.kind === 'database' ? s : null;
      // a reading without a check keeps what the last check found while it is the same fetch of the same connection (an edit elsewhere in the document is not a new fetch)
      setStatus((prev) => !check && db && prev && prev.fetchedAt === db.fetchedAt && prev.connection === db.connection ? { ...db, changed: prev.changed, checkError: prev.checkError } : db);
      return db;
    } catch { setStatus(null); return null; }
  }, [doc?.path, doc?.folio]);
  // read again whenever the document's information is (an edit that touches the label or the source: import, refresh, the refresh mode, undo, redo)
  useEffect(() => { void load(false); }, [doc?.path, key, info]);
  const loadConnections = () => dbConnections().then((r) => { setConnections(r.connections); setCredentialStore(r.credentialStore); }, () => setConnections([]));
  useEffect(() => { void loadConnections(); }, [doc?.path]);
  // a connection was made, changed or removed (the connection window): the document's source is judged against the list again ("this computer has no such connection")
  useEffect(() => { void load(false); }, [connections]);

  /**
   * The password question (the engine said `needsPassword`): the dialog opens, and the answer is `true` once the engine kept a right password for
   * this session (the screen keeps nothing); `false` when it was cancelled.
   */
  const askPassword = (connection: string): Promise<boolean> => new Promise((resolve) => {
    setPw((open) => { open?.resolve(false); return { connection, resolve }; });
  });
  const answered = (unlocked: boolean) => {
    const open = pw;
    setPw(null);
    open?.resolve(unlocked);
    if (unlocked) { setLocked((l) => (l === open?.connection ? null : l)); void loadConnections(); }
  };
  /** `run`, and when the engine says `needsPassword`: the question, then `run` again (a cancelled question is the refusal). */
  const withPassword = async <T,>(connection: string, run: () => Promise<T>): Promise<T> => {
    try { return await run(); } catch (e) {
      if (!(e instanceof DbError) || e.kind !== 'needsPassword') throw e;
      if (!(await askPassword(connection))) throw e;
      return run();
    }
  };

  /** The same text for every way the query is refused: the easy sentence and the engine's own words. */
  const problem = (e: unknown): string => e instanceof DbError ? `${explain(e.kind)} (${e.message})` : String(e);

  const fetchedMessage = async (): Promise<string> => {
    const s = await load(false);
    const r = s?.lastRefresh;
    return r ? tr('데이터 원본을 새로 고쳤습니다: {0}행 (늘어난 행 {1}, 줄어든 행 {2}, 바뀐 행 {3})', { 0: r.rows, 1: r.added, 2: r.removed, 3: r.changed }) : tr('데이터 원본을 가져왔습니다.');
  };

  /** Import: the query is tried first (a refusal is explained in the dialog), then the engine's edit makes the snapshot and the label. */
  const importQuery = async (q: Query, values?: Record<string, unknown>): Promise<{ ok: true } | { ok: false; error: string }> => {
    try {
      const probe = await withPassword(q.connection, () => dbPreview(q, values, 1));
      if (probe.count !== null && probe.count !== undefined && probe.count > 50000) return { ok: false, error: `${explain('limit')} (${probe.count})` };
    } catch (e) { return { ok: false, error: problem(e) }; }
    const r = await editRef.current.edit(tr('데이터베이스 원본 선택'), [{ op: 'dataMergeSelectDatabase', query: q, param_values: values, today: undefined, source: undefined, file_type: undefined, state: undefined }]);
    if (!r) return { ok: false, error: tr('가져오지 못했습니다. 이유는 화면 아래의 알림에 있습니다.') };
    editRef.current.flash(await fetchedMessage());
    return { ok: true };
  };

  /** Run `q` (the query the document remembers, as the status gave it) again; the values are the question's answer. */
  const refreshNow = async (q: Query | undefined, values?: Record<string, unknown>, quiet = false): Promise<string | null> => {
    if (!q) return tr('문서에 데이터베이스 원본이 없습니다.');
    try {
      // opening a document never asks: without a password the panel says "password needed · unlock" (`quiet`)
      if (quiet) await dbPreview(q, values, 1); else await withPassword(q.connection, () => dbPreview(q, values, 1));
    } catch (e) {
      if (e instanceof DbError && e.kind === 'needsPassword') setLocked(q.connection);
      return quiet && e instanceof DbError && e.kind === 'needsPassword' ? null : problem(e);
    }
    const r = await editRef.current.edit(tr('데이터베이스 원본 새로 고침'), [{ op: 'dataMergeRefresh', param_values: values ?? undefined, today: undefined, source: undefined, file_type: undefined, state: undefined }]);
    if (!r) return tr('새로 고치지 못했습니다. 이유는 화면 아래의 알림에 있습니다.');
    editRef.current.flash(await fetchedMessage());
    return null;
  };

  const askThenRun = (s: DatabaseStatus, what: Ask) => {
    const params = s.query.params;
    if (!params.length && what.message === undefined) { void what.run().then((e) => { if (e) flash(e); }); return; }
    setAsk({ ...what, error: '', busy: false, params, last: s.paramValues ?? {} });
  };
  const refresh = () => { if (status) askThenRun(status, { title: tr('데이터 원본 새로 고침'), ok: tr('새로 고침'), run: (values) => refreshNow(status.query, values) }); };

  const check = async (asked = false): Promise<void> => {
    setChecking(true);
    const s = await load(true);
    setChecking(false);
    if (s?.checkError?.errorKind === 'needsPassword' && !asked) {
      setLocked(s.connection);
      if (await askPassword(s.connection)) return check(true);
    }
    if (s?.checkError) flash(`${explain(s.checkError.errorKind)} (${s.checkError.error})`);
    else if (s && s.changed === false) flash(tr('데이터베이스의 내용이 가져온 것과 같습니다.'));
  };

  const setRefreshMode = (mode: Refresh) => { if (status && editable) void edit(tr('데이터 원본 새로 고침 방식'), [{ op: 'setDataMergeRefreshMode', mode }]); };

  // opening the document: the refresh mode the document remembers (ask: the question when the data changed, like the link update notice; automatic:
  // the defaults of the parameters again, so "this month" is this month; manual: nothing). It never asks for a password: a connection without one is
  // `locked` (the panel says "password needed · unlock", and unlocking does what the mode says), and the document opens as it is.
  const onOpen = async (s: DatabaseStatus, live: () => boolean) => {
    if (!s.connectionPresent || !editRef.current.editable) return;
    const mode = s.query.refresh;
    if (mode === 'autoOnOpen') {
      const e = await refreshNow(s.query, Object.fromEntries(s.query.params.map((p) => [p.name, p.default])), true);
      if (e) editRef.current.flash(e);
    } else if (mode === 'askOnOpen') {
      const c = await load(true);
      if (c?.checkError?.errorKind === 'needsPassword') { setLocked(c.connection); return; }
      if (!live() || !c || c.changed !== true) return;
      askThenRun(c, { title: tr('데이터 원본이 바뀌었습니다'), message: tr('데이터베이스 연결 "{0}" 의 내용이 문서에 가져온 것과 달라졌습니다. 새로 고칠까요?', { 0: c.connection }), ok: tr('새로 고침'), run: (values) => refreshNow(c.query, values) });
    }
  };
  const opened = useRef('');
  useEffect(() => {
    if (!doc?.folio || opened.current === doc.path) return;
    opened.current = doc.path;
    let live = true;
    void (async () => {
      const s = await load(false);
      if (!live || !s) return;
      await onOpen(s, () => live);
    })();
    return () => { live = false; };
  }, [doc?.path, doc?.folio]);
  // a document of another connection: nothing is locked until the engine says so
  useEffect(() => { if (locked && status && status.connection !== locked) setLocked(null); }, [status?.connection]);

  /** "Password needed · unlock": the question, then what the refresh mode says for an opened document. */
  const unlock = () => {
    const s = status;
    if (!s) return;
    void askPassword(s.connection).then(async (ok) => {
      if (!ok) return;
      setLocked(null);
      if ((s.query.refresh ?? 'manual') === 'manual') return;
      await onOpen(s, () => true);
    });
  };
  const lock = () => { if (status) void lockConnection(status.connection).then(() => loadConnections(), () => undefined); };

  const connect = () => {
    const first = connections[0];
    setQuery({ initial: status?.query ?? newQuery(first?.name ?? ''), title: status ? tr('데이터베이스 원본 바꾸기') : tr('데이터베이스에 연결') });
    if (!connections.length) setManage({});
  };
  const editQuery = () => { if (status) setQuery({ initial: status.query, title: tr('데이터베이스 원본 바꾸기') }); };

  const actions: DatabaseActions = { available, editable, status, connections, checking, locked: !!status && locked === status.connection, unlock, lock, connect,
    manageConnections: (fill) => setManage({ fill }), editQuery, refresh, check: () => void check(), setRefreshMode };

  const dialog = <>
    {query && <QueryDialog key={query.initial.connection + (status ? 'edit' : 'new')} initial={query.initial} title={query.title} connections={connections} sqliteFiles={sqliteFiles} askPassword={askPassword}
      computed={info?.dataMergeComputed ?? []} lastValues={status?.paramValues ?? {}} editable={editable}
      openConnections={(fill, select) => setManage({ fill, select })} onImport={importQuery} close={() => setQuery(null)} />}
    {manage && <ConnectionDialog start={manage.fill ? { fill: manage.fill } : manage.select ? { select: manage.select } : undefined} sqliteFiles={sqliteFiles} askPassword={askPassword}
      changed={setConnections} close={() => setManage(null)} />}
    {pw && <PasswordPrompt key={pw.connection} connection={pw.connection} store={credentialStore} onDone={answered} />}
    {ask && <AskParams params={ask.params} initial={ask.last} title={ask.title} message={ask.message} ok={ask.ok} busy={ask.busy} error={ask.error}
      onCancel={() => setAsk(null)}
      onOk={(values) => { setAsk((a) => a && { ...a, busy: true, error: '' }); void ask.run(values).then((e) => { if (e) setAsk((a) => a && { ...a, busy: false, error: e }); else setAsk(null); }); }} />}
  </>;

  return { actions, dialog, connect };
}
