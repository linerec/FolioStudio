import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { Command } from './commands';
import type { TextFormatting } from './textformat';
import { fieldKey } from './keys';
import { TextNumberField, formatNumeric, parseNumeric } from './controls/lengthfield';

type Context = { documentPath?: string; target?: string; edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null> };
type Settings = { enabled: boolean; size: number };

export function useOpticalMarginEnabled(story: string | undefined, revision: unknown) {
  const [value, setValue] = useState<{ story: string; enabled: boolean } | null>(null);
  useEffect(() => {
    let current = true;
    setValue(null);
    if (story) void window.folio.request({ cmd: 'opticalMargin', story }).then(({ header }) => {
      if (current && header.ok) setValue({ story, enabled: (header.settings as Settings).enabled });
    }).catch(() => {});
    return () => { current = false; };
  }, [story, revision]);
  return value?.story === story && value?.enabled === true;
}

export function useStoryPanel(control: TextFormatting, context: Context) {
  const [story, setStory] = useState<string | null>(null);
  useEffect(() => setStory(null), [context.documentPath]);
  const target = control.selection?.story ?? context.target;
  const handlers: Record<string, Partial<Command>> = {
    'type.story': { run: target ? () => setStory(target) : undefined },
    'id.panel.paragraph.ignoreOpticalMargin': { checked: control.selection?.format.ignoreEdgeAlignment === true,
      run: control.selection && control.opticalMarginEnabled ? () => control.apply({ ignoreEdgeAlignment: control.selection?.format.ignoreEdgeAlignment !== true }) : undefined },
  };
  return { handlers, dialog: story && <StoryPanel key={story} story={story} edit={context.edit} close={() => setStory(null)} /> };
}

function StoryPanel({ story, edit, close }: { story: string; edit: Context['edit']; close: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [size, setSize] = useState('');
  const [error, setError] = useState(''), [busy, setBusy] = useState(false);
  useEffect(() => {
    let current = true;
    void window.folio.request({ cmd: 'opticalMargin', story }).then(({ header }) => {
      if (!current) return;
      if (!header.ok) { setError(tr('스토리 설정을 읽지 못했습니다.')); return; }
      const value = header.settings as Settings;
      setSettings(value); setSize(String(value.size));
    }).catch(() => { if (current) setError(tr('스토리 설정을 읽지 못했습니다.')); });
    return () => { current = false; };
  }, [story]);
  const submit = async () => {
    if (!settings) return;
    setBusy(true); setError('');
    try {
      const result = await edit(tr('광학 여백 정렬'), [{ op: 'setOpticalMargin', story, props: { enabled: settings.enabled, size: Number(size) } }]);
      if (result) close(); else setError(tr('광학 여백 크기는 0.1–1296pt로 입력하세요.'));
    } catch { setError(tr('광학 여백 크기는 0.1–1296pt로 입력하세요.')); }
    finally { setBusy(false); }
  };
  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('스토리')} onPointerDown={e => e.stopPropagation()} onKeyDown={e => {
    e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close();
  }}><form className="card dialog style-options" onSubmit={e => { e.preventDefault(); if (!busy) void submit(); }}>
    <h2>{tr('스토리')}</h2>
    <label><input type="checkbox" aria-label={tr('광학 여백 정렬')} disabled={!settings || busy} checked={settings?.enabled ?? false} onChange={e => setSettings(v => v && { ...v, enabled: e.target.checked })} />{tr('광학 여백 정렬')}</label>
    <label>{tr('광학 여백 글자 크기 (pt)')}<TextNumberField autoFocus width={180} label={tr('광학 여백 글자 크기 (pt)')} disabled={!settings || busy} text={size} onText={setSize} format={formatNumeric} parse={parseNumeric} min={0.1} max={1296} step={1} /></label>
    <p className="muted">{tr('첫 글자와 끝 글자는 줄 배치에 반영됩니다.')}</p>
    {error && <p role="alert">{error}</p>}
    <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={!settings || busy}>{tr('확인')}</button></div>
  </form></div>, document.body);
}
