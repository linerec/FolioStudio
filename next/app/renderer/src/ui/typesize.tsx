import { useState } from 'react';
import { t as tr } from '../i18n';
import { fieldKey } from './keys';
import { NumberField } from './controls/numberfield';
import './typesize.css';

export function TypeSizeDialog({ initial, apply, close }: {
  initial: number | undefined; apply: (pointSize: number) => void; close: () => void;
}) {
  // the size as a number; none (a blank box) until one is chosen when the selection has no common size
  const [size, setSize] = useState<number | null>(initial == null ? null : initial);
  const valid = size !== null && Number.isFinite(size) && size >= 0.1 && size <= 1296;
  return <div className="style-modal" role="dialog" aria-modal="true" aria-label={tr('글꼴 크기 입력')}
    onPointerDown={e => e.stopPropagation()} onKeyDown={e => {
      e.stopPropagation();
      if (fieldKey(e) === 'cancel') { e.preventDefault(); close(); }
    }}>
    <form className="card dialog style-options type-size" onSubmit={e => {
      e.preventDefault();
      if (valid) { apply(size!); close(); }
    }}>
      <h2>{tr('글꼴 크기 입력')}</h2>
      <label>{tr('글꼴 크기 (pt)')}<NumberField value={size} label={tr('글꼴 크기 (pt)')} min={0.1} max={1296} step={1} decimals={4} width={96}
        autoFocus anchor={12} inDialog onCommit={setSize} /></label>
      <p>{tr('0.1~1296 pt 사이의 크기를 입력하세요.')}</p>
      <div className="dialog-actions">
        <button type="button" onClick={close}>{tr('취소')}</button>
        <button type="submit" disabled={!valid}>{tr('확인')}</button>
      </div>
    </form>
  </div>;
}
