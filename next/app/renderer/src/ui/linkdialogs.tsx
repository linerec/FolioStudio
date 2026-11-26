// Links panel menu dialogs: Relink File Extension… (the picked link takes the file of the same
// name, in its folder, with another extension; the file is looked for by linkmenu.ts).
import { useState } from 'react';
import type { LinkRow } from '../engine';
import { tr } from '../i18n';
import { type LinkColumns, withExtension } from './linkmenu';
import { Modal } from './tables';

export function RelinkExtensionDialog({ link, apply, close }: { link: LinkRow; apply: (link: LinkRow, ext: string) => Promise<void>; close: () => void }) {
  const [text, setText] = useState('');
  const ext = text.trim().replace(/^\.+/, '');
  const ok = /^[A-Za-z0-9]{1,10}$/.test(ext);
  return (
    <Modal title={tr('파일 확장자 다시 연결')} close={close} ok={ok} submit={() => void apply(link, ext)}>
      <fieldset><legend>{tr('확장자')}</legend>
        <label>{tr('새 확장자:')}<input aria-label={tr('새 확장자')} autoFocus value={text} onChange={(e) => setText(e.target.value)} /></label>
        <p className="p-empty small">{ok ? tr('연결할 파일: {0}', { 0: withExtension(link.path ?? link.recorded, ext) }) : tr('같은 폴더의 같은 이름 파일 중 확장자만 다른 것에 다시 연결합니다.')}</p>
      </fieldset>
    </Modal>
  );
}

/** Links panel menu ▸ Panel Options… and Link Options…: which columns the list shows (name always). */
export function LinksPanelOptionsDialog({ columns, apply, close, thumbnails, setThumbnails }: { columns: LinkColumns; apply: (c: LinkColumns) => void; close: () => void;
  thumbnails?: boolean; setThumbnails?: (v: boolean) => void }) {
  const [value, setValue] = useState(columns);
  const [thumbs, setThumbs] = useState(thumbnails ?? true);
  return (
    <Modal title={tr('링크 패널 옵션')} close={close} ok submit={() => { apply(value); setThumbnails?.(thumbs); }}>
      <fieldset><legend>{tr('열')}</legend>
        <label><input type="checkbox" aria-label={tr('상태 열')} checked={value.status} onChange={(e) => setValue({ ...value, status: e.target.checked })} />{tr('상태 열')}</label>
        <label><input type="checkbox" aria-label={tr('쪽 열')} checked={value.page} onChange={(e) => setValue({ ...value, page: e.target.checked })} />{tr('쪽 열')}</label>
      </fieldset>
      {setThumbnails && <fieldset><legend>{tr('섬네일')}</legend>
        <label><input type="checkbox" aria-label={tr('이름 열에 섬네일 표시')} checked={thumbs} onChange={(e) => setThumbs(e.target.checked)} />{tr('이름 열에 섬네일 표시')}</label>
      </fieldset>}
    </Modal>
  );
}
