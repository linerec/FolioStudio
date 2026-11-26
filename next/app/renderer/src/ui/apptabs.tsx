// The tab strip of the app windows that were gathered as tabs of one window (Window > Arrange > Consolidate All Windows): a tab per
// document with its name, a mark for unsaved changes and a close button. A click shows that document's window; a tab dragged along the
// strip changes its place; a tab pulled off the strip floats as a window of its own. The state is the shell's (ui/appwindows.ts).
import { useEffect, useRef, useState } from 'react';
import { t as tr } from '../i18n';
import { Icon } from './icons';
import { type AppWindows, dropIndex, pulledOff, showsTabs } from './appwindows';

/** A drag shorter than this is a click (pixels). */
const DRAG_FROM = 5;

export function AppTabs({ aw }: { aw: AppWindows }) {
  const { view, apply } = aw;
  const strip = useRef<HTMLDivElement | null>(null);
  // the tab being dragged and where the pointer is over the strip: `at` is the place it would take
  const [drag, setDrag] = useState<{ doc: number; at: number } | null>(null);
  const press = useRef<{ doc: number; x: number; y: number; moved: boolean } | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;

  const centres = (without: number) => Array.from(strip.current?.querySelectorAll<HTMLElement>('[data-app-tab]') ?? [])
    .filter((el) => Number(el.dataset.appTab) !== without)
    .map((el) => { const r = el.getBoundingClientRect(); return r.left + r.width / 2; });

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const p = press.current;
      if (!p) return;
      if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_FROM) return;
      p.moved = true;
      setDrag({ doc: p.doc, at: dropIndex(centres(p.doc), e.clientX) });
    };
    const up = (e: PointerEvent) => {
      const p = press.current;
      press.current = null;
      setDrag(null);
      if (!p || !p.moved) return;
      const r = strip.current?.getBoundingClientRect();
      if (r && pulledOff(r, e.clientX, e.clientY)) {
        // dropped away from the strip: a window of its own, with its corner at the pointer
        void apply({ cmd: 'floatDoc', doc: p.doc, at: { x: Math.round(e.screenX - 80), y: Math.round(e.screenY - 16) } });
      } else {
        void apply({ cmd: 'moveTab', doc: p.doc, index: dropIndex(centres(p.doc), e.clientX) });
      }
    };
    const cancel = (e: KeyboardEvent) => { if (e.key === 'Escape' && press.current) { press.current = null; setDrag(null); } };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('keydown', cancel);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); window.removeEventListener('keydown', cancel); };
  }, [apply]);

  if (!showsTabs(view)) return null;
  const others = view.tabs.filter((t) => t.id !== drag?.doc);
  return (
    <div className="app-tabs" ref={strip} role="tablist" aria-label={tr('문서 탭')}>
      {view.tabs.map((t) => {
        const place = drag && drag.doc !== t.id ? others.findIndex((o) => o.id === t.id) : -1;
        return (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.active}
            data-app-tab={t.id}
            className={`app-tab${t.active ? ' on' : ''}${drag?.doc === t.id ? ' dragging' : ''}${place === drag?.at ? ' drop-before' : ''}${drag && drag.at === others.length && place === others.length - 1 ? ' drop-after' : ''}`}
            title={tr('끌어서 순서를 바꾸고, 탭 줄 밖으로 끌어내면 따로 뜨는 창이 됩니다')}
            onPointerDown={(e) => { if (e.button === 0 && !(e.target as HTMLElement).closest('button')) press.current = { doc: t.id, x: e.clientX, y: e.clientY, moved: false }; }}
            onClick={() => { if (!t.active) void apply({ cmd: 'activate', doc: t.id }); }}
          >
            <span>{t.title ?? tr('시작 화면')}{t.dirty ? <b className="app-tab-dirty" title={tr('저장하지 않은 변경')}>*</b> : null}</span>
            <button className="doc-tab-x" onClick={(e) => { e.stopPropagation(); void apply({ cmd: 'closeDoc', doc: t.id }); }} title={tr('이 탭을 닫습니다')}><Icon name="close" size={11} /></button>
          </div>
        );
      })}
    </div>
  );
}
