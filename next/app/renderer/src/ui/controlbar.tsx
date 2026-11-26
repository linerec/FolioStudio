import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { t as tr } from '../i18n';
import { userSettings } from '../user-settings';
import { Icon } from './icons';

/** The control bar's place and the object groups it shows (build-c2): Dock at Top / Dock at Bottom / Float and Customize, kept in the
 *  engine's user settings (`folio.controlBar.dock` top | bottom | float, `folio.controlBar.at` "x,y" of a floating bar,
 *  `folio.controlBar.hidden` the hidden groups' keys, comma-separated), so an MCP client can read and set them too. */
export type ControlDock = 'top' | 'bottom' | 'float';
const DOCK = 'folio.controlBar.dock', AT = 'folio.controlBar.at', HIDDEN = 'folio.controlBar.hidden';

/** The groups of the object control bar, in the bar's order (`data-cp` on each). */
export const CONTROL_GROUPS: [string, () => string][] = [
  ['reference', () => tr('기준점')], ['position', () => tr('X·Y 위치')], ['size', () => tr('폭·높이')], ['sizeChain', () => tr('폭과 높이 비율 유지')],
  ['scale', () => tr('X·Y 비율')], ['scaleChain', () => tr('비율 유지')], ['rotate', () => tr('회전·기울이기 각도')], ['turnFlip', () => tr('90° 회전·뒤집기')],
  ['select', () => tr('컨테이너·내용·이전·다음 개체 선택')], ['paint', () => tr('칠·획 견본')], ['stroke', () => tr('획 두께·유형')], ['effects', () => tr('불투명도·효과')],
  ['wrap', () => tr('텍스트 감싸기')], ['corners', () => tr('모퉁이')], ['fitting', () => tr('맞춤')], ['align', () => tr('정렬')], ['alignTo', () => tr('정렬 기준')],
  ['objectStyle', () => tr('개체 스타일')],
];

/** The bars Customize… works on (rest-c1): the object bar (the Selection tool), the character and paragraph bars (the Type tool) and the
 *  page bar (the Page tool); each keeps its hidden groups in its own user setting (`folio.controlBar.hidden`, `….character`,
 *  `….paragraph`, `….page`). */
export type ControlBarKind = 'object' | 'character' | 'paragraph' | 'page';
export const BAR_GROUPS: Record<ControlBarKind, [string, () => string][]> = {
  object: CONTROL_GROUPS,
  character: [['char.font', () => tr('글꼴·글꼴 스타일')], ['char.size', () => tr('글꼴 크기·행간')], ['char.tracking', () => tr('자간·기준선 이동')],
    ['char.scale', () => tr('세로·가로 비율')], ['char.skew', () => tr('기울기·언어')], ['char.rotation', () => tr('글자 회전')], ['char.kerning', () => tr('커닝')],
    ['char.buttons', () => tr('대문자·첨자·밑줄·취소선 단추')], ['char.style', () => tr('문자 스타일')]],
  paragraph: [['para.align', () => tr('정렬 단추')], ['para.indent', () => tr('왼쪽·오른쪽 들여쓰기')], ['para.space', () => tr('첫 줄 들여쓰기·앞 공백')],
    ['para.spaceAfter', () => tr('마지막 줄 들여쓰기·뒤 공백')], ['para.dropCap', () => tr('장식 문자')], ['para.hyphenate', () => tr('하이픈 넣기')],
    ['para.composer', () => tr('컴포저')], ['para.optical', () => tr('광학 여백 무시')], ['para.style', () => tr('단락 스타일')]],
  page: [['page.reference', () => tr('기준점')], ['page.position', () => tr('X·Y 위치')], ['page.size', () => tr('폭·높이')], ['page.preset', () => tr('페이지 크기·방향')],
    ['page.options', () => tr('개체 함께 이동·기준 페이지 오버레이')], ['page.liquid', () => tr('액체 페이지 규칙')], ['page.info', () => tr('페이지 번호')]],
};
const HIDDEN_OF: Record<ControlBarKind, string> = { object: HIDDEN, character: HIDDEN + '.character', paragraph: HIDDEN + '.paragraph', page: HIDDEN + '.page' };
const BAR_SELECTOR: Record<ControlBarKind, string> = { object: '.control', character: '.control.text-format-bar', paragraph: '.control.text-format-bar', page: '.control.page-bar' };

let version = 0;
const listeners = new Set<() => void>();
const emit = () => { version++; listeners.forEach((f) => f()); };
const read = (key: string) => { try { return userSettings.getItem(key); } catch { return null; } };
const write = (key: string, value: string) => { try { userSettings.setItem(key, value); } catch { /* the engine keeps the last one */ } emit(); };
let watching = false;

export const controlBar = {
  dock(): ControlDock { const v = read(DOCK); return v === 'bottom' || v === 'float' ? v : 'top'; },
  at(): [number, number] {
    const [x, y] = (read(AT) ?? '').split(',').map(Number);
    return Number.isFinite(x) && Number.isFinite(y) ? [x, y] : [160, 120];
  },
  hidden(bar: ControlBarKind = 'object'): string[] { return (read(HIDDEN_OF[bar]) ?? '').split(',').filter((k) => BAR_GROUPS[bar].some(([g]) => g === k)); },
  setDock(d: ControlDock) { write(DOCK, d); },
  setAt(x: number, y: number) { write(AT, `${Math.round(x)},${Math.round(y)}`); },
  setHidden(keys: string[], bar: ControlBarKind = 'object') { write(HIDDEN_OF[bar], keys.join(',')); },
};

function subscribe(f: () => void) {
  listeners.add(f);
  // a change made elsewhere (an MCP client, another window) arrives as the engine's user-settings event
  if (!watching && window.folio?.onEvent) { watching = true; window.folio.onEvent((e) => { if (e.event === 'user-settings') window.setTimeout(emit, 0); }); }
  return () => { listeners.delete(f); };
}

/** The control bar settings, read again when they change. */
export function useControlBar() {
  useSyncExternalStore(subscribe, () => version);
  return { dock: controlBar.dock(), at: controlBar.at(), hidden: controlBar.hidden(), character: controlBar.hidden('character'), paragraph: controlBar.hidden('paragraph'), page: controlBar.hidden('page') };
}

/** CSS that hides the groups `hidden` names (the groups stay in the bar's code; only what shows changes). */
export function HiddenGroups({ hidden, bar = 'object' }: { hidden: string[]; bar?: ControlBarKind }) {
  if (!hidden.length) return null;
  return <style>{hidden.map((k) => `${BAR_SELECTOR[bar]} [data-cp="${k}"]{display:none}`).join('')}</style>;
}

/** The control bar menu at the bar's end: Dock at Top, Dock at Bottom, Float and Customize… (the groups shown of this bar). */
export function ControlBarMenu({ customizable = true, bar = 'object' }: { customizable?: boolean; bar?: ControlBarKind }) {
  const { dock } = useControlBar();
  const hidden = controlBar.hidden(bar);
  const groups = BAR_GROUPS[bar];
  const [open, setOpen] = useState<'menu' | 'customize' | null>(null);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(null); };
    window.addEventListener('pointerdown', away, true);
    return () => window.removeEventListener('pointerdown', away, true);
  }, [open]);
  const item = (label: string, on: boolean, run: () => void, field: string) =>
    <button type="button" role="menuitemradio" aria-checked={on} data-control-dock={field} onClick={() => { run(); setOpen(null); }}>{on ? '✓ ' : '  '}{label}</button>;
  return <div className="cp-group narrow cp-menu" ref={box}>
    <button type="button" className="cp-btn" title={tr('제어 막대 메뉴')} aria-label={tr('제어 막대 메뉴')} aria-haspopup="menu" aria-expanded={!!open}
      onClick={() => setOpen((v) => (v ? null : 'menu'))}><Icon name="chevronDown" size={14} /></button>
    {open === 'menu' && <div className="cp-popup" role="menu" aria-label={tr('제어 막대 메뉴')}>
      {item(tr('위에 붙이기'), dock === 'top', () => controlBar.setDock('top'), 'top')}
      {item(tr('아래에 붙이기'), dock === 'bottom', () => controlBar.setDock('bottom'), 'bottom')}
      {item(tr('떠 있기'), dock === 'float', () => controlBar.setDock('float'), 'float')}
      <hr />
      <button type="button" role="menuitem" data-control-customize disabled={!customizable} title={customizable ? undefined : tr('선택 도구의 제어 막대에서 고릅니다.')}
        onClick={() => setOpen('customize')}>{tr('사용자 지정…')}</button>
    </div>}
    {open === 'customize' && <div className="cp-popup cp-customize" role="dialog" aria-label={tr('제어 막대 사용자 지정')}>
      <p className="cp-popup-title">{tr('제어 막대에 보일 항목')}</p>
      {groups.map(([k, label]) => <label key={k}><input type="checkbox" data-control-group={k} checked={!hidden.includes(k)}
        onChange={(e) => controlBar.setHidden(e.target.checked ? hidden.filter((h) => h !== k) : [...hidden, k], bar)} /> {label()}</label>)}
      <div className="cp-popup-actions">
        <button type="button" onClick={() => controlBar.setHidden([], bar)}>{tr('모두 보이기')}</button>
        <button type="button" onClick={() => setOpen(null)}>{tr('닫기')}</button>
      </div>
    </div>}
  </div>;
}

/** A floating control bar: at its place, moved by the grip at its left end. */
export function FloatingControl({ children }: { children: React.ReactNode }) {
  const { at } = useControlBar();
  const [drag, setDrag] = useState<[number, number] | null>(null);
  const start = useRef<{ x: number; y: number; at: [number, number] } | null>(null);
  const shown = drag ?? at;
  const clamp = (x: number, y: number): [number, number] => [Math.max(0, Math.min(x, window.innerWidth - 120)), Math.max(0, Math.min(y, window.innerHeight - 40))];
  return <div className="control-float" style={{ left: shown[0], top: shown[1] }} data-control-float>
    <div className="control-float-grip" title={tr('끌어서 제어 막대를 옮깁니다')}
      onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); start.current = { x: e.clientX, y: e.clientY, at: shown }; }}
      onPointerMove={(e) => { const s = start.current; if (s) setDrag(clamp(s.at[0] + e.clientX - s.x, s.at[1] + e.clientY - s.y)); }}
      onPointerUp={() => { if (drag) controlBar.setAt(drag[0], drag[1]); start.current = null; setDrag(null); }} />
    {children}
  </div>;
}
