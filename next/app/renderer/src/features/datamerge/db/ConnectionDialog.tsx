import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../../../i18n';
import { fieldKey } from '../../../ui/keys';
import { DbError, dbCapabilities, dbConnections, lockConnection, removeConnection, saveConnection, testConnection, unlockConnection, type Connection, type ConnectionSettings, type Driver, type TlsMode } from './api';
import { DRIVERS, defaultPort, driverName, emptyConnection, explain, fromHint, storeUnavailableText, toSettings } from './model';
import './db.css';

export type TestOutcome = { state: 'idle' } | { state: 'testing' } | { state: 'ok'; version: string; ms: number } | { state: 'failed'; kind: string; detail: string; fingerprint?: string };

/** The easy explanation of a failed test, with the engine's own sentence as the detail and, for a certificate that is not trusted, what to do. */
export function TestResult({ outcome, trust }: { outcome: TestOutcome; trust?: (fingerprint: string) => void }) {
  if (outcome.state === 'idle') return null;
  if (outcome.state === 'testing') return <p role="status" data-db-test="testing">{tr('연결을 시험하는 중…')}</p>;
  if (outcome.state === 'ok') return <p role="status" data-db-test="ok">{tr('연결했습니다. 서버: {0} ({1}ms)', { 0: outcome.version, 1: outcome.ms })}</p>;
  return <div role="alert" data-db-test="failed" data-db-error-kind={outcome.kind}>
    <p>{explain(outcome.kind)}</p>
    <small className="db-detail">{outcome.detail}</small>
    {outcome.kind === 'tls' && outcome.fingerprint && <div className="db-trust" data-db-fingerprint={outcome.fingerprint}>
      <p>{tr('서버가 보낸 인증서의 지문(SHA-256)입니다. 직접 만든 서버라면 서버에서 같은 지문인지 확인한 뒤 믿을 수 있습니다.')}</p>
      <code>{outcome.fingerprint}</code>
      {trust && <button type="button" data-db-trust onClick={() => trust(outcome.fingerprint!)}>{tr('이 인증서 신뢰')}</button>}
    </div>}
  </div>;
}

export function failedOutcome(e: unknown): TestOutcome {
  if (e instanceof DbError) return { state: 'failed', kind: e.kind, detail: e.message, fingerprint: e.fingerprint };
  return { state: 'failed', kind: 'unknown', detail: String(e) };
}

/**
 * The connections of this computer (user settings: never the project; the passwords are in the operating system's credential store): the saved
 * ones, a form for a new one or one being changed, and the test. The engine does all of it (`dbConnections`, `dbSaveConnection`,
 * `dbRemoveConnection`, `dbTestConnection`); a document that points at a connection this computer does not have opens the form filled in from
 * what the document knows of it (`start`).
 */
export function ConnectionDialog({ start, sqliteFiles, close, changed, askPassword }: {
  /** A connection to select (by name), or the form to fill in for a connection the document names. */
  start?: { select?: string; fill?: ConnectionSettings };
  /** The files of the project that may be SQLite databases (root-relative paths). */
  sqliteFiles: string[];
  close: () => void;
  /** The list of saved connections changed. */
  changed: (list: Connection[]) => void;
  /** Ask for a saved connection's password (the engine said `needsPassword`); resolves `true` once the engine kept it for the session. */
  askPassword: (connection: string) => Promise<boolean>;
}) {
  const [list, setList] = useState<Connection[]>([]);
  const [store, setStore] = useState(true);
  // why passwords cannot be saved on this device (dbCapabilities; dbplatform-c1): the greyed "save the password" says it
  const [storeReason, setStoreReason] = useState<string | undefined>();
  useEffect(() => { dbCapabilities().then((c) => setStoreReason(c.credentialStore.reason), () => {}); }, []);
  const [selected, setSelected] = useState<string | null>(start?.select ?? null);
  const [form, setForm] = useState<ConnectionSettings>(() => start?.fill ?? emptyConnection());
  const [isNew, setIsNew] = useState(!start?.select);
  const [test, setTest] = useState<TestOutcome>({ state: 'idle' });
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState('');
  const serial = useRef(0);

  const reload = async (select?: string | null) => {
    try {
      const r = await dbConnections();
      setList(r.connections);
      setStore(r.credentialStore);
      changed(r.connections);
      const pick = select === undefined ? selected : select;
      const found = r.connections.find((c) => c.name === pick);
      if (found) { setSelected(found.name); setForm(toSettings(found)); setIsNew(false); }
    } catch (e) { setTest(failedOutcome(e)); }
  };
  useEffect(() => { void reload(start?.select); }, []);

  const set = (patch: Partial<ConnectionSettings>) => { setForm((f) => ({ ...f, ...patch })); setTest({ state: 'idle' }); setNote(''); };
  const setDriver = (driver: Driver) => set({ driver, port: defaultPort(driver), host: driver === 'sqlite' ? '' : form.host, user: driver === 'sqlite' ? '' : form.user, instance: undefined });
  // a named instance finds its port through the SQL Server Browser: the default port gives way (a port typed by hand stays)
  const setInstance = (instance: string) => set({ instance: instance || undefined, port: instance && form.port === defaultPort(form.driver) ? undefined : !instance && form.port === undefined ? defaultPort(form.driver) : form.port });
  const sqlite = form.driver === 'sqlite';
  const complete = !!form.name.trim() && !!form.database.trim() && (sqlite || (!!form.host.trim() && !!form.user.trim()));

  const choose = (c: Connection) => { setSelected(c.name); setForm(toSettings(c)); setIsNew(false); setTest({ state: 'idle' }); setNote(''); };
  const create = () => { setSelected(null); setForm(emptyConnection()); setIsNew(true); setTest({ state: 'idle' }); setNote(''); };

  const runTest = async (asked = false) => {
    const mine = ++serial.current;
    setTest({ state: 'testing' });
    try {
      const r = await testConnection(form);
      if (serial.current === mine) setTest({ state: 'ok', version: r.serverVersion, ms: r.ms });
    } catch (e) {
      if (serial.current !== mine) return;
      // a saved connection whose password was not saved and none was typed: the password is asked for (once), then the test goes on
      if (e instanceof DbError && e.kind === 'needsPassword' && !asked && !isNew && selected && !form.password) {
        setTest({ state: 'idle' });
        if (await askPassword(selected)) { await reload(); await runTest(true); return; }
      }
      setTest(failedOutcome(e));
    }
  };
  const save = async () => {
    setBusy(true);
    try {
      await saveConnection(form);
      let remembered = false;
      let rememberFailure: unknown = null;
      // a password typed but not to be saved: the engine checks it and keeps it for this session (the screen keeps nothing)
      if (form.password && !form.savePassword && form.driver !== 'sqlite') {
        try { await unlockConnection(form.name, form.password, false); remembered = true; } catch (e) { rememberFailure = e; }
      }
      setNote(remembered ? tr('연결 "{0}" 을(를) 저장했고 비밀번호를 이 프로그램을 켜 둔 동안 기억합니다.', { 0: form.name }) : tr('연결 "{0}" 을(를) 저장했습니다.', { 0: form.name }));
      await reload(form.name);
      if (rememberFailure) setTest(failedOutcome(rememberFailure));
    } catch (e) { setTest(failedOutcome(e)); } finally { setBusy(false); }
  };
  const forget = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await lockConnection(selected);
      setNote(tr('연결 "{0}" 의 비밀번호를 잊었습니다.', { 0: selected }));
      await reload();
    } catch (e) { setTest(failedOutcome(e)); } finally { setBusy(false); }
  };
  const unlockNow = async () => {
    if (!selected) return;
    if (await askPassword(selected)) { setNote(tr('연결 "{0}" 의 비밀번호를 기억합니다.', { 0: selected })); await reload(); }
  };
  const remove = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await removeConnection(selected);
      setNote(tr('연결 "{0}" 을(를) 지웠습니다.', { 0: selected }));
      setSelected(null); setForm(emptyConnection()); setIsNew(true);
      await reload(null);
    } catch (e) { setTest(failedOutcome(e)); } finally { setBusy(false); }
  };
  const trust = (fingerprint: string) => { setForm((f) => ({ ...f, tls: { mode: 'verify', trustFingerprint: fingerprint } })); setTest({ state: 'idle' }); setNote(tr('이 인증서를 믿도록 정했습니다. 저장하면 이 컴퓨터에서 이 서버의 이 인증서만 믿습니다.')); };

  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('데이터베이스 연결')} data-db-connection-dialog onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close(); }}>
      <div className="card dialog db-dialog db-connections">
        <h2>{tr('데이터베이스 연결')}</h2>
        <p className="db-explain">{tr('연결 설정은 이 컴퓨터에만 저장됩니다. 프로젝트 폴더에는 연결 이름만 남고 주소·계정·비밀번호는 남지 않습니다.')}</p>
        <div className="db-columns">
          <div className="db-side">
            <ul className="db-list" aria-label={tr('저장된 연결')} data-db-connection-list>
              {list.map((c) => <li key={c.name} data-db-connection={c.name} aria-selected={c.name === selected && !isNew}>
                <button type="button" onClick={() => choose(c)}>
                  <b>{c.name}</b>
                  <small>{driverName(c.driver)}{c.driver === 'sqlite' ? ` · ${c.database}` : ` · ${c.host} · ${c.database}`}</small>
                  <small data-db-password-state={c.driver === 'sqlite' ? 'none' : c.hasPassword ? 'saved' : c.unlocked ? 'session' : 'unsaved'}>{c.driver === 'sqlite' ? '' : c.hasPassword ? tr('비밀번호 저장됨') : c.unlocked ? tr('비밀번호 기억 중 (이 세션)') : tr('비밀번호 저장 안 됨')}</small>
                </button>
              </li>)}
              {!list.length && <li className="db-none">{tr('저장된 연결이 없습니다.')}</li>}
            </ul>
            <button type="button" data-db-new-connection onClick={create}>{tr('새 연결')}</button>
          </div>
          <fieldset className="db-form" disabled={busy}>
            <legend>{isNew ? tr('새 연결') : tr('연결 편집')}</legend>
            <label>{tr('연결 이름')}
              <input aria-label={tr('연결 이름')} data-db-field="name" value={form.name} readOnly={!isNew} onChange={(e) => set({ name: e.target.value })} />
            </label>
            {!isNew && <small>{tr('저장한 연결의 이름은 바꿀 수 없습니다. 문서가 이름으로 연결을 가리킵니다.')}</small>}
            <label>{tr('데이터베이스 종류')}
              <select aria-label={tr('데이터베이스 종류')} data-db-field="driver" value={form.driver} onChange={(e) => setDriver(e.target.value as Driver)}>
                {DRIVERS.map((d) => <option key={d} value={d}>{driverName(d)}</option>)}
              </select>
            </label>
            {sqlite ? <>
              <label>{tr('프로젝트 안의 SQLite 파일')}
                <select aria-label={tr('프로젝트 안의 SQLite 파일')} data-db-field="sqliteFile" value={sqliteFiles.includes(form.database) ? form.database : ''} onChange={(e) => set({ database: e.target.value })}>
                  <option value="">{sqliteFiles.length ? tr('(고르세요)') : tr('(프로젝트에 SQLite 파일이 없습니다)')}</option>
                  {sqliteFiles.map((f) => <option key={f} value={f}>{f}</option>)}
                </select>
              </label>
              <label>{tr('파일 경로(프로젝트 폴더 기준)')}
                <input aria-label={tr('파일 경로(프로젝트 폴더 기준)')} data-db-field="database" value={form.database} onChange={(e) => set({ database: e.target.value })} />
              </label>
              <small>{tr('SQLite 파일은 프로젝트 폴더 안에 두어야 하고 읽기 전용으로만 엽니다.')}</small>
            </> : <>
              <label>{tr('호스트')}
                <input aria-label={tr('호스트')} data-db-field="host" value={form.host} onChange={(e) => set({ host: e.target.value })} />
              </label>
              <label>{tr('포트')}
                <input aria-label={tr('포트')} data-db-field="port" inputMode="numeric" value={form.port ?? ''} placeholder={form.driver === 'sqlserver' && form.instance ? tr('(SQL Server Browser 에게 묻기)') : String(defaultPort(form.driver) ?? '')}
                  onChange={(e) => { const n = parseInt(e.target.value, 10); set({ port: Number.isFinite(n) && n > 0 ? Math.min(65535, n) : undefined }); }} />
              </label>
              {form.driver === 'sqlserver' && <>
                <label>{tr('인스턴스 이름(선택)')}
                  <input aria-label={tr('인스턴스 이름(선택)')} data-db-field="instance" value={form.instance ?? ''} placeholder="SQLEXPRESS" onChange={(e) => setInstance(e.target.value.trim())} />
                </label>
                <small>{tr('이름 있는 인스턴스에 연결할 때만 씁니다. 포트를 비워 두면 SQL Server Browser(UDP 1434)에게 포트를 묻습니다. Azure SQL 은 인스턴스 없이 포트 1433 입니다.')}</small>
              </>}
              <label>{tr('데이터베이스 이름')}
                <input aria-label={tr('데이터베이스 이름')} data-db-field="database" value={form.database} onChange={(e) => set({ database: e.target.value })} />
              </label>
              <label>{tr('사용자')}
                <input aria-label={tr('사용자')} data-db-field="user" autoComplete="off" value={form.user} onChange={(e) => set({ user: e.target.value })} />
              </label>
              <label>{tr('비밀번호')}
                <input aria-label={tr('비밀번호')} data-db-field="password" type="password" autoComplete="new-password" value={form.password ?? ''}
                  placeholder={selected && list.find((c) => c.name === selected)?.hasPassword ? tr('저장됨 (바꾸려면 새로 입력)') : ''} onChange={(e) => set({ password: e.target.value })} />
              </label>
              <label className="check" title={store ? undefined : storeUnavailableText(storeReason)} data-db-store-reason={store ? undefined : storeReason ?? 'none'}>
                <input type="checkbox" aria-label={tr('비밀번호 저장')} data-db-field="savePassword" checked={!!form.savePassword && store} disabled={!store} onChange={(e) => set({ savePassword: e.target.checked })} />
                {tr('비밀번호 저장')}
              </label>
              <small>{store ? tr('비밀번호는 운영체제의 자격 증명 저장소에 저장되고 설정 파일이나 프로젝트에는 남지 않습니다. 저장하지 않으면 이 프로그램을 켜 둔 동안만 기억하고, 가져오기나 새로 고침에 비밀번호가 필요하면 그때 물어봅니다.')
                : storeUnavailableText(storeReason)}</small>
              {!isNew && selected && !sqlite && (() => {
                const c = list.find((x) => x.name === selected);
                if (!c) return null;
                return c.unlocked && !c.hasPassword
                  ? <button type="button" data-db-forget onClick={() => void forget()}>{tr('기억한 비밀번호 잊기')}</button>
                  : !c.hasPassword ? <button type="button" data-db-unlock onClick={() => void unlockNow()}>{tr('비밀번호 입력…')}</button> : null;
              })()}
              <label>{tr('보안 연결(TLS)')}
                <select aria-label={tr('보안 연결(TLS)')} data-db-field="tls" value={form.tls.mode} onChange={(e) => set({ tls: { mode: e.target.value as TlsMode, trustFingerprint: e.target.value === 'verify' ? form.tls.trustFingerprint : undefined } })}>
                  <option value="verify">{tr('켜고 인증서 확인 (권장)')}</option>
                  <option value="require">{tr('켜되 인증서는 확인하지 않음')}</option>
                  <option value="disable">{tr('끄기 (암호화하지 않음)')}</option>
                </select>
              </label>
              {form.tls.mode === 'disable' && <small role="note" className="db-warn">{tr('TLS 를 끄면 계정 정보와 데이터가 암호화되지 않고 오갑니다. 같은 컴퓨터나 믿을 수 있는 내부망에서만 쓰세요.')}</small>}
              {form.tls.mode === 'require' && <small role="note" className="db-warn">{tr('인증서를 확인하지 않으면 암호화는 되지만 접속한 서버가 진짜인지는 알 수 없습니다.')}</small>}
              {form.tls.trustFingerprint && <div className="db-trusted" data-db-trusted={form.tls.trustFingerprint}>
                <small>{tr('믿기로 한 인증서 지문: {0}', { 0: form.tls.trustFingerprint })}</small>
                <button type="button" onClick={() => set({ tls: { mode: 'verify' } })}>{tr('신뢰 해제')}</button>
              </div>}
            </>}
            <TestResult outcome={test} trust={form.driver === 'sqlite' ? undefined : trust} />
            {note && <p role="status" data-db-note>{note}</p>}
          </fieldset>
        </div>
        <div className="dialog-actions">
          <button type="button" data-db-delete disabled={busy || !selected || isNew} onClick={() => void remove()}>{tr('삭제')}</button>
          <span className="db-spacer" />
          <button type="button" data-db-test-connection disabled={busy || !complete || test.state === 'testing'} onClick={() => void runTest()}>{tr('연결 시험')}</button>
          <button type="button" data-db-save disabled={busy || !complete} onClick={() => void save()}>{tr('저장')}</button>
          <button type="button" data-db-close disabled={busy} onClick={close}>{tr('닫기')}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

export { fromHint };
