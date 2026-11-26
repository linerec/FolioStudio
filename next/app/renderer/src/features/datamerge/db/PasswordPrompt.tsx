import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../../../i18n';
import { fieldKey } from '../../../ui/keys';
import { DbError, dbCapabilities, unlockConnection } from './api';
import { explain, storeUnavailableText } from './model';
import './db.css';

/**
 * The question for a connection's password when the engine says `needsPassword` (the password was not saved on this computer): what the password
 * is for, a field, and how long to remember it: this session (the engine's memory; gone when the program quits) or the operating system's credential
 * store. The engine checks the password against the server (`dbUnlockConnection`) and keeps only a right one; a wrong one is explained here and the
 * question stays. The password goes to the engine and nowhere else: not a command that edits the document, not a setting, not the project.
 */
export function PasswordPrompt({ connection, store, onDone }: {
  connection: string;
  /** This computer has a credential store to save into. */
  store: boolean;
  /** `true` once the engine kept the password; `false` when the question was cancelled. */
  onDone: (unlocked: boolean) => void;
}) {
  const [password, setPassword] = useState('');
  const [remember, setRemember] = useState<'session' | 'store'>('session');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ kind: string; detail: string } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const [storeReason, setStoreReason] = useState<string | undefined>();
  useEffect(() => { if (!store) dbCapabilities().then((c) => setStoreReason(c.credentialStore.reason), () => {}); }, [store]);
  useEffect(() => { input.current?.focus(); }, []);

  const submit = async () => {
    if (!password || busy) return;
    setBusy(true); setError(null);
    try {
      await unlockConnection(connection, password, remember === 'store' && store);
      setPassword('');
      onDone(true);
    } catch (e) {
      setBusy(false);
      setError(e instanceof DbError ? { kind: e.kind, detail: e.message } : { kind: 'unknown', detail: String(e) });
    }
  };

  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('비밀번호 입력')} data-db-password-prompt data-db-connection-name={connection} onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) { setPassword(''); onDone(false); } }}>
      <form className="card dialog db-dialog db-ask" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
        <h2>{tr('비밀번호 입력')}</h2>
        <p className="db-explain" data-db-password-message>{tr('연결 "{0}" 의 비밀번호를 이 컴퓨터에 저장하지 않았습니다. 비밀번호를 입력하세요.', { 0: connection })}</p>
        <label>{tr('비밀번호')}
          <input ref={input} type="password" aria-label={tr('비밀번호')} data-db-password-input autoComplete="off" value={password} disabled={busy}
            onChange={(e) => { setPassword(e.target.value); setError(null); }} />
        </label>
        <fieldset className="db-remember" disabled={busy}>
          <legend>{tr('얼마 동안 기억할까요?')}</legend>
          <label className="check"><input type="radio" name="db-remember" data-db-remember="session" checked={remember === 'session'} onChange={() => setRemember('session')} />{tr('이 프로그램을 켜 둔 동안만')}</label>
          <label className="check"><input type="radio" name="db-remember" data-db-remember="store" checked={remember === 'store' && store} disabled={!store} onChange={() => setRemember('store')} />{tr('운영체제의 자격 증명 저장소에 저장')}</label>
          <small>{store ? tr('비밀번호는 문서·설정 파일·기록에 남지 않습니다. 프로그램을 끄면 기억한 비밀번호를 잊습니다.') : storeUnavailableText(storeReason)}</small>
        </fieldset>
        {error && <div role="alert" className="db-error" data-db-password-error data-db-error-kind={error.kind}>
          <p>{explain(error.kind)}</p>
          <small className="db-detail">{error.detail}</small>
        </div>}
        <div className="dialog-actions">
          <span className="db-spacer" />
          <button type="button" data-db-password-cancel disabled={busy} onClick={() => { setPassword(''); onDone(false); }}>{tr('취소')}</button>
          <button type="submit" data-db-password-ok disabled={busy || !password}>{busy ? tr('확인하는 중…') : tr('확인')}</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
