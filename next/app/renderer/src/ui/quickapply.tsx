import { useEffect, useRef, useState } from 'react';
import { t as tr } from '../i18n';
import { isEnabled, type Commands } from './commands';
import { quickApplyKey } from './keys';
import type { TextFormatting } from './textformat';
import './quickapply.css';

type Entry = { id: string; name: string; category: string; run: () => void };

export function QuickApply({ commands, text, objectStyle, close }: {
  commands: Commands; text: TextFormatting; objectStyle?: (id: string) => void; close: () => void;
}) {
  const [query, setQuery] = useState(''), [index, setIndex] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  const entries: Entry[] = Object.entries(commands)
    .filter(([id, c]) => id !== 'edit.quickApply' && typeof c.label === 'string' && c.label.length > 0 && c.run && isEnabled(c))
    .map(([id, c]) => ({ id, name: c.label, category: tr('메뉴 명령'), run: c.run! }));
  for (const kind of ['paragraph', 'character'] as const) {
    if (!text.selection) continue;
    const styles = kind === 'paragraph' ? text.info?.paragraphStyles : text.info?.characterStyles;
    for (const style of styles ?? []) entries.push({ id: style.id, name: [...style.group, style.name].join(' / '),
      category: kind === 'paragraph' ? tr('단락 스타일') : tr('문자 스타일'), run: () => text.style(kind, style.id, false) });
  }
  if (objectStyle) for (const style of text.info?.objectStyles ?? []) entries.push({
    id: style.id, name: [...style.group, style.name].join(' / '), category: tr('개체 스타일'), run: () => objectStyle(style.id),
  });
  const words = query.toLocaleLowerCase().trim().split(/\s+/);
  const matches = entries.filter(e => words.every(w => e.name.toLocaleLowerCase().includes(w)));
  const selected = Math.min(index, Math.max(0, matches.length - 1));
  useEffect(() => { list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }); }, [selected, query]);
  const apply = (entry: Entry | undefined) => { if (entry) { close(); entry.run(); } };
  return <div className="style-modal" role="dialog" aria-modal="true" aria-label={tr('빠른 적용')}
    onPointerDown={e => { e.stopPropagation(); if (e.target === e.currentTarget) close(); }}
    onKeyDown={e => {
      e.stopPropagation(); const action = quickApplyKey(e.nativeEvent); if (!action) return;
      e.preventDefault();
      if (action === 'close') close();
      if (action === 'apply') apply(matches[selected]);
      if (action === 'previous') setIndex(Math.max(0, selected - 1));
      if (action === 'next') setIndex(Math.max(0, Math.min(matches.length - 1, selected + 1)));
    }}>
    <div className="card dialog quick-apply">
      <h2>{tr('빠른 적용')}</h2>
      <input autoFocus aria-label={tr('명령과 스타일 검색')} role="combobox" aria-expanded="true"
        aria-controls="quick-apply-results" aria-activedescendant={matches.length ? `quick-apply-${selected}` : undefined}
        value={query} onChange={e => { setQuery(e.target.value); setIndex(0); }}/>
      <div ref={list} id="quick-apply-results" role="listbox" aria-label={tr('명령과 스타일 검색')} className="quick-apply-results">
        {matches.map((entry, i) => <button key={entry.id} id={`quick-apply-${i}`} type="button" role="option"
          tabIndex={-1} aria-selected={i === selected} onPointerDown={e => e.preventDefault()} onClick={() => apply(entry)}>
          <span>{entry.name}</span><small>{entry.category}</small>
        </button>)}
      </div>
      {!matches.length && <p role="status">{tr('일치하는 명령이나 스타일이 없습니다.')}</p>}
      <div className="dialog-actions"><button type="button" onClick={close}>{tr('닫기')}</button></div>
    </div>
  </div>;
}
