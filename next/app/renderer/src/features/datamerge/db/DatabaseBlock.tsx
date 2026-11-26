import { tr } from '../../../i18n';
import type { DatabaseActions } from './useDatabaseSource';
import { REFRESH_MODES, driverName, fromHint, paramText, refreshLabel, shortTime } from './model';
import './db.css';

/**
 * The Data Merge panel's part for a database: the button that connects one, and, when the document's source is a database, what the document
 * remembers of it (connection, table, when the rows were fetched, how many, the values of the parameters), whether the data in the database has
 * changed since, and what to do: refresh, check, change the query, the refresh mode. A connection this computer does not have is said, with the
 * button for the connection settings (the form is filled in from what the document knows of it); the snapshot is still what merges and prints.
 */
export function DatabaseBlock({ db }: { db: DatabaseActions }) {
  const s = db.status;
  if (!s) {
    return <section className="datamerge-db" data-db-status="none">
      <button type="button" data-db-connect disabled={!db.available} onClick={db.connect}>{tr('데이터베이스에 연결…')}</button>
      <button type="button" data-db-manage onClick={() => db.manageConnections()}>{tr('연결 설정…')}</button>
    </section>;
  }
  const present = s.connectionPresent;
  const table = s.query.sql ? tr('SQL (고급)') : `${s.query.table?.schema ? `${s.query.table.schema}.` : ''}${s.query.table?.name ?? ''}`;
  const values = Object.entries(s.paramValues ?? {});
  return <section className="datamerge-db" data-db-status={present ? 'present' : 'missing'} data-db-connection-name={s.connection}>
    <h3>{tr('데이터베이스 원본')}</h3>
    <dl>
      <dt>{tr('연결')}</dt>
      <dd data-db-connection-name-text>{s.connection}{s.hint ? ` · ${driverName(s.hint.driver)}` : ''}</dd>
      <dt>{tr('테이블')}</dt><dd data-db-table-text>{table}</dd>
      <dt>{tr('가져온 시각')}</dt><dd data-db-fetched>{shortTime(s.fetchedAt)}</dd>
      <dt>{tr('행 수')}</dt><dd data-db-rows>{s.rows ?? ''}</dd>
      {values.length > 0 && <><dt>{tr('매개변수 값')}</dt><dd data-db-param-values>{values.map(([k, v]) => `${k}=${paramText(v)}`).join(', ')}</dd></>}
    </dl>
    {!present && <div role="status" className="db-warn" data-db-missing>
      <p>{tr('연결 "{0}" 이(가) 이 컴퓨터에 없습니다. 마지막으로 가져온 값으로 병합·인쇄합니다.', { 0: s.connection })}</p>
      <button type="button" data-db-setup onClick={() => db.manageConnections(fromHint(s.connection, s.hint))}>{tr('연결 설정')}</button>
    </div>}
    {present && db.locked && <div role="status" className="db-warn" data-db-locked>
      <p>{tr('비밀번호 필요: 연결 "{0}" 의 비밀번호가 이 컴퓨터에 없어 데이터베이스에 닿지 못했습니다. 마지막으로 가져온 값으로 병합·인쇄합니다.', { 0: s.connection })}</p>
      <button type="button" data-db-unlock onClick={db.unlock}>{tr('비밀번호 필요 · 잠금 해제')}</button>
    </div>}
    {present && !db.locked && db.connections.find((c) => c.name === s.connection)?.unlocked && !db.connections.find((c) => c.name === s.connection)?.hasPassword &&
      <p data-db-unlocked>{tr('비밀번호를 이 프로그램을 켜 둔 동안 기억 중입니다.')} <button type="button" data-db-lock onClick={db.lock}>{tr('잊기')}</button></p>}
    {s.changed === true && <p role="alert" className="db-changed" data-db-changed>{tr('바뀜: 데이터베이스의 내용이 문서에 가져온 것과 다릅니다.')}</p>}
    {s.changed === false && <p role="status" data-db-unchanged>{tr('데이터베이스의 내용이 가져온 것과 같습니다.')}</p>}
    {s.lastRefresh && <p data-db-last-refresh>{tr('마지막 새로 고침: 늘어난 행 {0}, 줄어든 행 {1}, 바뀐 행 {2}', { 0: s.lastRefresh.added, 1: s.lastRefresh.removed, 2: s.lastRefresh.changed })}</p>}
    <div className="datamerge-db-actions">
      <button type="button" data-db-refresh-now disabled={!present || !db.editable} title={!present ? tr('이 컴퓨터에 연결이 없어 새로 고칠 수 없습니다.') : undefined} onClick={db.refresh}>{tr('새로 고침')}</button>
      <button type="button" data-db-check disabled={!present || db.checking} onClick={db.check}>{db.checking ? tr('확인하는 중…') : tr('변경 확인')}</button>
      <button type="button" data-db-edit-query onClick={db.editQuery}>{tr('질의 편집…')}</button>
      <button type="button" data-db-manage onClick={() => db.manageConnections()}>{tr('연결 설정…')}</button>
    </div>
    <label className="datamerge-db-mode">{tr('새로 고침 방식')}
      <select aria-label={tr('새로 고침 방식')} data-db-refresh-mode value={s.query.refresh ?? 'manual'} disabled={!db.editable} onChange={(e) => db.setRefreshMode(e.target.value as typeof REFRESH_MODES[number])}>
        {REFRESH_MODES.map((m) => <option key={m} value={m}>{refreshLabel(m)}</option>)}
      </select>
    </label>
  </section>;
}
