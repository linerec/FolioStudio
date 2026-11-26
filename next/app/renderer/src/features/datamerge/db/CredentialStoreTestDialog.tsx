import { useEffect, useState } from 'react';
import { tr, useLanguage } from '../../../i18n';
import { credentialStoreSelfTest, type SelfTestResult } from './api';
import { explain, storeKindName, storeUnavailableText } from './model';
import './db.css';

const STEP_NAMES: Record<string, () => string> = {
  save: () => tr('시험 항목 저장'),
  read: () => tr('다시 읽어 같은지 확인'),
  remove: () => tr('지우기'),
  gone: () => tr('지운 뒤 남지 않았는지 확인'),
};

/**
 * Help > Diagnostics > 비밀번호 저장소 시험… (dbplatform-c1): the engine keeps a throwaway entry in this computer's credential store, reads it back,
 * removes it and checks it is gone (`dbCredentialStoreSelfTest`). No connection is touched and the value never comes back. On a Mac this proves the
 * keychain, on a Linux desktop the Secret Service, where this PC cannot test them.
 */
export function CredentialStoreTestDialog({ onClose }: { onClose: () => void }) {
  useLanguage();
  const [result, setResult] = useState<SelfTestResult | null>(null);
  const [failure, setFailure] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true); setFailure('');
    try { setResult(await credentialStoreSelfTest()); } catch (e) { setResult(null); setFailure(e instanceof Error ? e.message : String(e)); }
    setBusy(false);
  };
  useEffect(() => { void run(); }, []);
  const state = busy ? 'running' : failure ? 'error' : !result ? 'running' : !result.store.available ? 'unavailable' : result.ok ? 'ok' : 'fail';
  return (
    <div className="overlay" onClick={onClose}>
      <div className="card dialog db-dialog db-credtest" data-credtest="dialog" data-credtest-result={state} onClick={(e) => e.stopPropagation()}>
        <h2>{tr('비밀번호 저장소 시험')}</h2>
        <p className="small">{tr('이 컴퓨터의 비밀번호 저장소에 버리는 시험 항목을 저장하고, 다시 읽고, 지운 뒤 남지 않았는지 확인합니다. 저장한 연결의 비밀번호는 건드리지 않습니다.')}</p>
        {result && <p data-credtest-store={result.store.kind}>{tr('저장소: {0}', { 0: storeKindName(result.store.kind) })}</p>}
        {result && !result.store.available && <p className="small" data-credtest-reason={result.store.reason ?? ''}>{storeUnavailableText(result.store.reason)}</p>}
        {result && result.steps.length > 0 && (
          <ol className="small">
            {result.steps.map((s) => (
              <li key={s.step} data-credtest-step={s.step} data-ok={s.ok ? '1' : '0'}>
                {s.ok ? '✓' : '✗'} {STEP_NAMES[s.step]?.() ?? s.step}{!s.ok && s.errorKind ? ` — ${explain(s.errorKind)}` : ''}
              </li>
            ))}
          </ol>
        )}
        <p data-credtest-summary="1">
          {state === 'running' ? tr('시험하는 중…')
            : state === 'ok' ? tr('통과: 이 컴퓨터에서 비밀번호를 저장하고 읽고 지울 수 있습니다 ({0} ms).', { 0: result?.ms ?? 0 })
            : state === 'unavailable' ? tr('이 컴퓨터에는 쓸 수 있는 비밀번호 저장소가 없습니다.')
            : state === 'fail' ? tr('실패: 비밀번호 저장소가 제대로 동작하지 않습니다. 비밀번호를 저장하지 말고 이 세션 동안만 기억하게 하세요.')
            : tr('시험을 실행하지 못했습니다: {0}', { 0: failure })}
        </p>
        <div className="actions">
          <button data-credtest="again" disabled={busy} onClick={() => void run()}>{tr('다시 시험')}</button>
          <button className="primary" data-credtest="close" onClick={onClose}>{tr('닫기')}</button>
        </div>
      </div>
    </div>
  );
}
