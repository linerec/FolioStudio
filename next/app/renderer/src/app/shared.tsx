import { t as tr } from '../i18n';
import { useEffect,useRef,useState } from 'react';
import { keyLabel } from '../ui/keys';
import {
type Anchor,type DocumentSummary
} from '../engine';
import { type Commands } from '../ui/commands';
import { type RefPoint } from '../ui/control';
import { queue } from '../ui/docview';
import { CONTEXT_MENUS,type MenuEntry } from '../ui/menudef';
import { ALL_TOOLS } from '../ui/tools';



/** Right-click menus on the page, InDesign's for the same situation (CONTEXT_MENUS), or a short fallback. */
export function contextItems(kind: string): MenuEntry[] {
  const fromInDesign = (CONTEXT_MENUS as Record<string, MenuEntry[] | undefined>)[kind];
  if (fromInDesign?.length) return fromInDesign;
  const zoom: MenuEntry[] = [{ cmd: 'view.zoomIn' }, { cmd: 'view.zoomOut' }, { cmd: 'view.fitPage' }, { cmd: 'view.fitSpread' }];
  if (kind === 'layoutNone') return [{ cmd: 'edit.paste' }, { cmd: 'edit.pasteInPlace' }, { sep: true }, ...zoom, { sep: true }, { cmd: 'view.rulers' }, { cmd: 'view.guides' }, { cmd: 'view.frameEdges' }];
  return [
    { cmd: 'edit.cut' }, { cmd: 'edit.copy' }, { cmd: 'edit.paste' }, { sep: true },
    ...zoom, { sep: true },
    { cmd: 'object.selectContainer' }, { cmd: 'object.selectContent' }, { cmd: 'view.fitSelection' }, { sep: true },
    { cmd: 'object.lock' }, { cmd: 'object.hide' }, { cmd: 'object.group' }, { cmd: 'object.ungroup' }, { sep: true },
    { cmd: 'object.effects' }, { cmd: 'object.textFrameOptions' },
  ];
}

export const sec = (ms?: number) => (ms === undefined ? '–' : (ms / 1000).toFixed(ms < 1000 ? 2 : 1));

export function Opening({ stage }: { stage: string }) {
  const STAGES: [string, string][] = [
    ['project', tr('프로젝트 참조 확인')], ['reader', tr('INDD 변환')],
    ['parse', tr('문서 해석')], ['fonts', tr('글꼴 찾기')], ['layout', tr('조판')],
  ];
  const at = STAGES.findIndex(([k]) => k === stage);
  return (
    <div className="overlay">
      <div className="card">
        <b>{tr('문서를 여는 중')}</b>
        <ol className="stages">
          {STAGES.map(([k, label], i) => <li key={k} className={i < at ? 'done' : i === at ? 'now' : ''}>{label}</li>)}
        </ol>
      </div>
    </div>
  );
}

/** InDesign's question when a changed document is closed. */
export function Unsaved({ name, onSave, onDiscard, onCancel }: { name: string; onSave: () => void; onDiscard: () => void; onCancel: () => void }) {
  return (
    <div className="overlay">
      <div className="card dialog">
        <h2>{tr('저장하지 않은 변경 내용')}</h2>
        <p>{tr('"{0}"을(를) 닫기 전에 변경 내용을 저장하시겠습니까?', {0: name})}</p>
        <div className="actions">
          <button onClick={onDiscard}>{tr('저장 안 함')}</button>
          <button onClick={onCancel}>{tr('취소')}</button>
          <button className="primary" onClick={onSave} autoFocus>{tr('저장')}</button>
        </div>
      </div>
    </div>
  );
}

/** The control panel's reference point as the engine's anchor name. */
export function anchorOf([fx, fy]: RefPoint): Anchor {
  const col = fx === 0 ? 'Left' : fx === 1 ? 'Right' : '';
  if (fy === 0.5) return col ? (col.toLowerCase() as Anchor) : 'center';
  const row = fy === 0 ? 'top' : 'bottom';
  return (col ? `${row}${col}` : row) as Anchor;
}

export function About({ onClose }: { onClose: () => void }) {
  const [v, setV] = useState('');
  useEffect(() => { window.folio.appInfo().then((i) => setV(i.version), () => {}); }, []);
  return (
    <div className="overlay" onClick={onClose}>
      <div className="card dialog" onClick={(e) => e.stopPropagation()}>
        <h2>Folio Studio <span>{v}</span></h2>
        <p className="muted">{tr('잡지·인쇄물을 만들어 인쇄소에 넘기는 전문 출판 도구. 화면은 InDesign의 "필수 클래식" 작업 영역을 따릅니다.')}</p>
        <div className="actions"><button className="primary" onClick={onClose}>{tr('닫기')}</button></div>
      </div>
    </div>
  );
}

export function Shortcuts({ commands, onClose }: { commands: Commands; onClose: () => void }) {
  const rows = Object.entries(commands).filter(([, c]) => c.keys?.length);
  return (
    <div className="overlay" onClick={onClose}>
      <div className="card dialog shortcuts" onClick={(e) => e.stopPropagation()}>
        <h2>{tr('단축키 ')}<span>{tr('InDesign 기본 세트와 같은 키')}</span></h2>
        <div className="sc-grid">
          {ALL_TOOLS.filter((t) => t.key).map((t) => (
            <div key={t.id} className="sc-row"><span>{tr(`tool.${t.id}`)}</span><kbd>{keyLabel(t.key)}</kbd></div>
          ))}
          {rows.map(([id, c]) => (
            <div key={id} className={`sc-row${c.run ? '' : ' off'}`}><span>{c.label}</span><kbd>{c.keys!.map(keyLabel).join(', ')}</kbd></div>
          ))}
        </div>
        <div className="actions"><button className="primary" onClick={onClose}>{tr('닫기')}</button></div>
      </div>
    </div>
  );
}

/** View ▸ Screen Mode ▸ Presentation: one spread at a time on black, arrows to move, Esc out. */
export function Presentation({ doc, rows, start, background, onKey: runKey, onExit }: { doc: DocumentSummary; rows: DocumentSummary['pages'][]; start: number; background: import('../ui/presentationkeys').PresentationBackground; onKey: (e: KeyboardEvent) => boolean; onExit: (page: number) => void }) {
  const [i, setI] = useState(() => Math.max(0, rows.findIndex((r) => r.some((p) => p.index === start))));
  const row = rows[i] ?? rows[0];
  const w = row.reduce((s, p) => s + p.width, 0), h = Math.max(...row.map((p) => p.height));
  const scale = Math.min((window.innerWidth - 40) / w, (window.innerHeight - 40) / h);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (runKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); return; }
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onExit(row[0].index); }
      if (['ArrowRight', 'ArrowDown', 'PageDown', ' '].includes(e.key)) { e.preventDefault(); e.stopPropagation(); setI((x) => Math.min(rows.length - 1, x + 1)); }
      if (['ArrowLeft', 'ArrowUp', 'PageUp'].includes(e.key)) { e.preventDefault(); e.stopPropagation(); setI((x) => Math.max(0, x - 1)); }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [rows.length, row, onExit, runKey]);
  void doc;
  return (
    <div className="presentation" style={{ background }} onClick={(e) => setI((x) => (e.shiftKey ? Math.max(0, x - 1) : Math.min(rows.length - 1, x + 1)))}>
      <div className="spread">{row.map((p) => <StaticPage key={p.index} index={p.index} width={p.width} height={p.height} scale={scale} />)}</div>
      <span className="presentation-hint">{tr('{0} / {1} · ← → 이동 · Esc 끝내기', {0: i + 1, 1: rows.length})}</span>
    </div>
  );
}

export function StaticPage({ index, width, height, scale }: { index: number; width: number; height: number; scale: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    let live = true;
    queue.render(index, Math.min(scale * (window.devicePixelRatio || 1), Math.sqrt(1.2e7 / (width * height))), -1).then((b) => {
      if (!live) return;
      c.width = b.width;
      c.height = b.height;
      c.getContext('2d')!.putImageData(new ImageData(b.pixels, b.width, b.height), 0, 0);
    }, () => {});
    return () => { live = false; };
  }, [index, scale, width, height]);
  return <canvas ref={ref} style={{ width: Math.round(width * scale), height: Math.round(height * scale) }} />;
}
