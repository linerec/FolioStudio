// Tools panel (left), in InDesign's order and groups with the same shortcut letters. A group
// shows its last used tool; press and hold, right-click or Alt+click opens the rest.
import { useEffect, useRef, useState } from 'react';
import { Icon } from './icons';
import { tr } from '../i18n';
import { useHelpCard } from './toolhelpview';
import type { HelpId } from './toolhelp';

export type ToolsLayout = 'SINGLE_COLUMN' | 'DOUBLE_COLUMN' | 'SINGLE_ROW';
export const toolsLayout = (value: unknown): ToolsLayout => value === 'SINGLE_COLUMN' || value === 'SINGLE_ROW' ? value : 'DOUBLE_COLUMN';

export type ToolId =
  | 'selection' | 'direct' | 'page' | 'gap' | 'collector' | 'placer'
  | 'type' | 'typePath' | 'typeVertical' | 'line' | 'pen' | 'penAdd' | 'penDelete' | 'convert' | 'pencil' | 'smooth' | 'erase'
  | 'rectFrame' | 'ellipseFrame' | 'polygonFrame' | 'rect' | 'ellipse' | 'polygon'
  | 'scissors' | 'freeTransform' | 'rotate' | 'scale' | 'shear' | 'gradient' | 'gradientFeather'
  | 'note' | 'eyedropper' | 'measure' | 'hand' | 'zoom';

export type ToolDef = { id: ToolId; label: string; key?: string; icon: string };

/** Sections (a gap between them), each a list of groups; a group's first tool is its default. */
export const TOOL_SECTIONS: ToolDef[][][] = [
  [
    [{ id: 'selection', get label() { return tr("선택 도구"); }, key: 'V', icon: 'selection' }],
    [{ id: 'direct', get label() { return tr("직접 선택 도구"); }, key: 'A', icon: 'direct' }],
  ],
  [
    [{ id: 'page', get label() { return tr("페이지 도구"); }, key: 'Shift+P', icon: 'page' }],
    [{ id: 'gap', get label() { return tr("간격 도구"); }, key: 'U', icon: 'gap' }],
    [
      { id: 'collector', get label() { return tr("내용 수집 도구"); }, key: 'B', icon: 'collector' },
      { id: 'placer', get label() { return tr("내용 배치 도구"); }, key: 'Shift+B', icon: 'collector' },
    ],
  ],
  [
    [
      { id: 'type', get label() { return tr("문자 도구"); }, key: 'T', icon: 'type' },
      { id: 'typePath', get label() { return tr("패스에 입력 도구"); }, key: 'Shift+T', icon: 'typePath' },
      { id: 'typeVertical', get label() { return tr("세로 문자 도구"); }, icon: 'type' },
    ],
    [{ id: 'line', get label() { return tr("선 도구"); }, key: '\\', icon: 'line' }],
    [
      { id: 'pen', get label() { return tr("펜 도구"); }, key: 'P', icon: 'pen' },
      { id: 'penAdd', get label() { return tr("고정점 추가 도구"); }, key: '=', icon: 'pen' },
      { id: 'penDelete', get label() { return tr("고정점 삭제 도구"); }, key: '-', icon: 'pen' },
      { id: 'convert', get label() { return tr("방향점 전환 도구"); }, key: 'Shift+C', icon: 'pen' },
    ],
    [
      { id: 'pencil', get label() { return tr("연필 도구"); }, key: 'N', icon: 'pencil' },
      { id: 'smooth', get label() { return tr("매끄럽게 도구"); }, icon: 'pencil' },
      { id: 'erase', get label() { return tr("지우개 도구"); }, icon: 'pencil' },
    ],
    [
      { id: 'rectFrame', get label() { return tr("사각형 프레임 도구"); }, key: 'F', icon: 'rectFrame' },
      { id: 'ellipseFrame', get label() { return tr("타원 프레임 도구"); }, icon: 'ellipseFrame' },
      { id: 'polygonFrame', get label() { return tr("다각형 프레임 도구"); }, icon: 'polygonFrame' },
    ],
    [
      { id: 'rect', get label() { return tr("사각형 도구"); }, key: 'M', icon: 'rect' },
      { id: 'ellipse', get label() { return tr("타원 도구"); }, key: 'L', icon: 'ellipse' },
      { id: 'polygon', get label() { return tr("다각형 도구"); }, icon: 'polygon' },
    ],
  ],
  [
    [{ id: 'scissors', get label() { return tr("가위 도구"); }, key: 'C', icon: 'scissors' }],
    [
      { id: 'freeTransform', get label() { return tr("자유 변형 도구"); }, key: 'E', icon: 'freeTransform' },
      { id: 'rotate', get label() { return tr("회전 도구"); }, key: 'R', icon: 'rotate' },
      { id: 'scale', get label() { return tr("크기 조정 도구"); }, key: 'S', icon: 'scale' },
      { id: 'shear', get label() { return tr("기울이기 도구"); }, key: 'O', icon: 'shear' },
    ],
    [{ id: 'gradient', get label() { return tr("그레이디언트 색상 견본 도구"); }, key: 'G', icon: 'gradient' }],
    [{ id: 'gradientFeather', get label() { return tr("그레이디언트 페더 도구"); }, key: 'Shift+G', icon: 'gradientFeather' }],
  ],
  [
    [{ id: 'note', get label() { return tr("메모 도구"); }, icon: 'note' }],
    [
      { id: 'eyedropper', get label() { return tr("스포이드 도구"); }, key: 'I', icon: 'eyedropper' },
      { id: 'measure', get label() { return tr("측정 도구"); }, key: 'K', icon: 'measure' },
    ],
    [{ id: 'hand', get label() { return tr("손 도구"); }, key: 'H', icon: 'hand' }],
    [{ id: 'zoom', get label() { return tr("확대/축소 도구"); }, key: 'Z', icon: 'zoom' }],
  ],
];

export const ALL_TOOLS: ToolDef[] = TOOL_SECTIONS.flat(2);
export const toolDef = (id: ToolId) => ALL_TOOLS.find((t) => t.id === id)!;

/** A line in a tool's tooltip and tool help card on what Folio does that the button cannot show (a key, a gesture). It never compares
 *  Folio with InDesign and never says what was not checked (user decision 2026-11-21: the screen does not explain "InDesign 과 다른 점"). */
/** The guides (the same words are in Preferences ▸ Guides & Pasteboard: tools/gen-preferences-dialog.py). */
export const guidesNote = () => tr('안내선을 Shift 를 누른 채 끌면 눈금에 붙습니다.');
/** The selection tools: the guides' note and the pasteboard between two spreads (an object let go there belongs to the spread its middle is nearer to). */
const selectionNote = () => `${guidesNote()}
${tr('개체를 펼침면 사이의 대지에 놓으면 개체의 가운데가 더 가까운 펼침면의 개체가 됩니다.')}`;
const TOOL_NOTES: Partial<Record<ToolId, () => string>> = {
  selection: selectionNote, direct: selectionNote,
  pen: () => tr('선택한 열린 패스의 끝점을 눌러 이어 그리고, 다른 열린 패스의 끝점을 눌러 합치고, 같은 패스의 다른 끝점을 눌러 닫습니다.'),
  pencil: () => tr('설정은 연필 도구를 두 번 눌러 엽니다.'),
  smooth: () => tr('패스에서 12pt 안쪽에서 끌기 시작하면 끈 부분을 매끄럽게 합니다.'),
};
/** The tool's note (the tool help card says it too: ui/toolhelpview.tsx). */
export const toolNote = (id: ToolId): string | undefined => TOOL_NOTES[id]?.();
const toolTip = (t: ToolDef) => { const base = `${t.label}${t.key ? ` (${t.key})` : ''}`, d = TOOL_NOTES[t.id]; return d ? `${base}\n${d()}` : base; };


export type ScreenMode = 'normal' | 'preview' | 'bleed' | 'slug' | 'presentation';
export type Swatchy = { rgb: [number, number, number] | null; none: boolean; name: string | null; /** Characters of different colours: the glyph shows `?` (T mode). */ mixed?: boolean };

type Props = {
  tool: ToolId;
  onTool: (t: ToolId) => void;
  onToolDoubleClick?: (t: ToolId) => void;
  /** Double-clicking the Fill (false) or Stroke (true) chip opens the Color Picker. */
  onOpenColorPicker?: (stroke: boolean) => void;
  double: boolean;
  onToggleDouble: () => void;
  layout: ToolsLayout;
  onLayout: (layout: ToolsLayout) => void;
  fill: Swatchy;
  stroke: Swatchy;
  strokeFocus: boolean;
  onFocusStroke: (stroke: boolean) => void;
  /** Swap Fill and Stroke, Default Fill and Stroke, Apply Color / Gradient / None (ui/toolboxpaint.ts); absent: nothing to apply to now. */
  onSwap?: () => void;
  onDefault?: () => void;
  onApplyColor?: () => void;
  onApplyGradient?: () => void;
  onApplyNone?: () => void;
  textMode: boolean;
  /** The widget shows the paint of the text (T glyphs): Formatting Affects Text with text frames selected, or characters selected. */
  textStyle?: boolean;
  onTextMode: (on: boolean) => void;
  screenMode: ScreenMode;
  onScreenMode: (m: ScreenMode) => void;
  /** `sidebar`: in the Folio workspace's sidebar, a grid whose columns follow the sidebar's width (the layout preference and its grip are
   *  InDesign Classic's floating Tools panel). The buttons, their titles, keys and flyouts are the same. */
  placement?: 'edge' | 'sidebar';
};

export function ToolsPanel(p: Props) {
  const [shown, setShown] = useState<Record<number, ToolId>>({});
  const [flyout, setFlyout] = useState<{ group: ToolDef[]; key: number; x: number; y: number } | null>(null);
  const hold = useRef(0);
  // the instant tool card (Preferences ▸ Interface ▸ Show Tool Help at Once): beside the tools, below a tools row
  const help = useHelpCard(p.layout === 'SINGLE_ROW' && p.placement !== 'sidebar' ? 'below' : 'right');

  // the group of the current tool shows it
  useEffect(() => {
    TOOL_SECTIONS.flat().forEach((g, i) => {
      if (g.some((t) => t.id === p.tool)) setShown((s) => (s[i] === p.tool ? s : { ...s, [i]: p.tool }));
    });
  }, [p.tool]);

  useEffect(() => {
    if (!flyout) return;
    const close = (e: MouseEvent) => { const t = e.target; if (!(t instanceof Element) || !t.closest('.tool-flyout')) setFlyout(null); };
    window.addEventListener('mousedown', close, true);
    return () => window.removeEventListener('mousedown', close, true);
  }, [flyout]);

  let groupIndex = -1;
  return (
    <aside className={`toolbox${p.placement === 'sidebar' ? ' sidebar-tools' : p.layout === 'DOUBLE_COLUMN' ? ' double' : p.layout === 'SINGLE_ROW' ? ' horizontal' : ''}`}>
      {p.placement !== 'sidebar' && <button className="toolbox-grip" onClick={() => p.onLayout(p.layout === 'DOUBLE_COLUMN' ? 'SINGLE_COLUMN' : 'DOUBLE_COLUMN')} title={p.layout === 'DOUBLE_COLUMN' ? tr('한 열로') : tr('두 열로')}>
        <Icon name={p.layout === 'DOUBLE_COLUMN' ? 'doubleLeft' : 'doubleRight'} size={12} />
      </button>}
      {TOOL_SECTIONS.map((section, si) => (
        <div className="tool-section" key={si}>
          {section.map((group) => {
            groupIndex++;
            const gi = groupIndex;
            const current = group.find((t) => t.id === (shown[gi] ?? group[0].id)) ?? group[0];
            const active = group.some((t) => t.id === p.tool);
            const openFly = (el: HTMLElement) => {
              const r = el.getBoundingClientRect();
              setFlyout({ group, key: gi, x: Math.min(p.layout === 'SINGLE_ROW' ? r.left : r.right + 2, window.innerWidth - 240), y: p.layout === 'SINGLE_ROW' ? r.bottom + 2 : r.top });
            };
            return (
              <button
                key={gi}
                className={`tool${active ? ' on' : ''}`}
                title={toolTip(current)}
                {...help.trigger(current.id)}
                onMouseDown={(e) => {
                  if (e.button !== 0) return;
                  const el = e.currentTarget;
                  if (group.length > 1) hold.current = window.setTimeout(() => openFly(el), 320);
                }}
                onMouseUp={() => window.clearTimeout(hold.current)}
                onMouseLeave={() => window.clearTimeout(hold.current)}
                onClick={(e) => {
                  if (flyout) return;
                  if (e.altKey && group.length > 1) {
                    const i = group.findIndex((t) => t.id === current.id);
                    p.onTool(group[(i + 1) % group.length].id);
                  } else p.onTool(current.id);
                }}
                onDoubleClick={() => p.onToolDoubleClick?.(current.id)}
                onContextMenu={(e) => { e.preventDefault(); if (group.length > 1) openFly(e.currentTarget); }}
              >
                <Icon name={current.icon} size={20} />
                {group.length > 1 && <i className="tool-more" />}
              </button>
            );
          })}
        </div>
      ))}
      <div className="tool-section colors">
        <FillStroke {...p} help={help.trigger} />
        <div className="apply-row">
          <button className={p.textMode ? 'mini' : 'mini on'} title={tr("컨테이너에 서식 적용 (J)")} onClick={() => p.onTextMode(false)} {...help.trigger('formatContainer')}><Icon name="container" size={14} /></button>
          <button className={p.textMode ? 'mini on' : 'mini'} title={tr("텍스트에 서식 적용 (J)")} onClick={() => p.onTextMode(true)} {...help.trigger('formatText')}><Icon name="textMode" size={14} /></button>
        </div>
        <div className="apply-row">
          <button className="mini" title={tr('색상 적용 (,)')} disabled={!p.onApplyColor} onClick={p.onApplyColor} {...help.trigger('applyColor')}><Icon name="applyColor" size={14} /></button>
          <button className="mini" title={tr('그레이디언트 적용 (.)')} disabled={!p.onApplyGradient} onClick={p.onApplyGradient} {...help.trigger('applyGradient')}><Icon name="applyGradient" size={14} /></button>
          <button className="mini" title={tr('없음 적용 (/)')} disabled={!p.onApplyNone} onClick={p.onApplyNone} {...help.trigger('applyNone')}><Icon name="applyNone" size={14} /></button>
        </div>
      </div>
      <div className="tool-section">
        <ScreenModeButton mode={p.screenMode} onMode={p.onScreenMode} horizontal={p.layout === 'SINGLE_ROW'} help={help.trigger} />
      </div>
      {flyout && (
        <div className="tool-flyout" style={{ left: flyout.x, top: flyout.y }}>
          {flyout.group.map((t) => (
            <button
              key={t.id}
              className={`${t.id === p.tool ? 'on' : ''}`}
              title={TOOL_NOTES[t.id] ? toolTip(t) : undefined}
              {...help.trigger(t.id, '.tool-flyout')}
              onClick={() => { setShown((s) => ({ ...s, [flyout.key]: t.id })); p.onTool(t.id); setFlyout(null); }}
            >
              <span className="fly-dot" aria-hidden="true" />
              <Icon name={t.icon} size={16} />
              <span className="fly-label">{t.label}</span>
              <span className="fly-key">{t.key ?? ''}</span>
            </button>
          ))}
        </div>
      )}
      {help.card}
    </aside>
  );
}

function chip(s: Swatchy): React.CSSProperties {
  if (s.none || !s.rgb) return { background: '#fff' };
  return { background: `rgb(${s.rgb.join(',')})` };
}

/** One box of the widget in T mode (InDesign 2025): a white square with a T, solid for the fill and outlined for the stroke; a red slash for none, grey with `?` when mixed. */
function GlyphBox({ s, outline }: { s: Swatchy; outline: boolean }) {
  const color = s.mixed ? '#c8c8c8' : s.rgb ? `rgb(${s.rgb.join(',')})` : '#000';
  return (
    <svg className="fs-glyph" viewBox="0 0 22 22" aria-hidden="true">
      {!s.none && <text x="11" y="18" textAnchor="middle" fontSize="19" fontWeight="bold" fontFamily="Times New Roman, serif"
        fill={outline ? 'none' : color} stroke={outline ? color : 'none'} strokeWidth={outline ? 1 : 0}>T</text>}
      {s.none && <text x="11" y="18" textAnchor="middle" fontSize="19" fontWeight="bold" fontFamily="Times New Roman, serif" fill="none" stroke="#ddd" strokeWidth="1">T</text>}
      {s.none && <line x1="1" y1="21" x2="21" y2="1" stroke="#e0302a" strokeWidth="1.5" />}
      {s.mixed && ['2,6', '20,6', '2,20', '20,20'].map((p) => <text key={p} x={p.split(',')[0]} y={p.split(',')[1]} textAnchor="middle" fontSize="7" fill="#444">?</text>)}
    </svg>
  );
}

type HelpTrigger = ReturnType<typeof useHelpCard>['trigger'];

function FillStroke({ fill, stroke, strokeFocus, textStyle, onFocusStroke, onSwap, onDefault, onOpenColorPicker, help }: Props & { help: HelpTrigger }) {
  const text = !!textStyle;
  return (
    <div className={`fillstroke${text ? ' text-style' : ''}`} title={tr("칠과 획 (X: 전환, Shift+X: 맞바꾸기, D: 기본값)")} data-text-style={text ? 'true' : 'false'}>
      <button className={`fs-fill${strokeFocus ? '' : ' front'}${!text && fill.none ? ' none' : ''}${text ? ' tx' : ''}`} style={text ? undefined : chip(fill)} onClick={() => onFocusStroke(false)} onDoubleClick={() => onOpenColorPicker?.(false)} {...help('fill')} title={tr('칠: {0}', {0:fill.mixed ? tr('여러 색 (혼합)') : fill.name ?? (fill.none ? tr('[없음]') : '')})}>
        {text && <GlyphBox s={fill} outline={false} />}
      </button>
      <button className={`fs-stroke${strokeFocus ? ' front' : ''}${!text && stroke.none ? ' none' : ''}${text ? ' tx' : ''}`} style={text ? undefined : { borderColor: stroke.none || !stroke.rgb ? '#fff' : `rgb(${stroke.rgb.join(',')})` }} onClick={() => onFocusStroke(true)} onDoubleClick={() => onOpenColorPicker?.(true)} {...help('stroke')} title={tr('획: {0}', {0:stroke.mixed ? tr('여러 색 (혼합)') : stroke.name ?? (stroke.none ? tr('[없음]') : '')})}>
        {text && <GlyphBox s={stroke} outline />}
      </button>
      <span className="fs-swap" role="button" aria-disabled={!onSwap} title={tr('칠과 획 맞바꾸기 (Shift+X)')} onClick={onSwap} {...help('swapFillStroke')}><Icon name="swapArrow" size={12} /></span>
      <span className="fs-default" role="button" aria-disabled={!onDefault} title={tr('기본 칠과 획 (D)')} onClick={onDefault} {...help('defaultFillStroke')} />
    </div>
  );
}

const MODE_HELP: Record<ScreenMode, HelpId> = { normal: 'screenNormal', preview: 'screenPreview', bleed: 'screenBleed', slug: 'screenSlug', presentation: 'screenPresentation' };
const MODES: { id: ScreenMode; label: string; icon: string; key?: string; working: boolean }[] = [
  { id: 'normal', get label() { return tr("표준"); }, icon: 'screenNormal', key: 'W', working: true },
  { id: 'preview', get label() { return tr("미리 보기"); }, icon: 'screenPreview', key: 'W', working: true },
  { id: 'bleed', get label() { return tr("도련"); }, icon: 'screenBleed', working: true },
  { id: 'slug', get label() { return tr("슬러그"); }, icon: 'screenBleed', working: true },
  { id: 'presentation', get label() { return tr("프레젠테이션"); }, icon: 'screenPresentation', key: 'Shift+W', working: true },
];

function ScreenModeButton({ mode, onMode, horizontal, help }: { mode: ScreenMode; onMode: (m: ScreenMode) => void; horizontal: boolean; help: HelpTrigger }) {
  const [open, setOpen] = useState<DOMRect | null>(null);
  const cur = MODES.find((m) => m.id === mode) ?? MODES[0];
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { const t = e.target; if (!(t instanceof Element) || !t.closest('.tool-flyout')) setOpen(null); };
    window.addEventListener('mousedown', close, true);
    return () => window.removeEventListener('mousedown', close, true);
  }, [open]);
  return (
    <>
      <button
        className="tool"
        title={tr('화면 모드: {0} (W)', {0:cur.label})}
        {...help('screenMode')}
        onClick={() => onMode(mode === 'normal' ? 'preview' : 'normal')}
        onContextMenu={(e) => { e.preventDefault(); setOpen(e.currentTarget.getBoundingClientRect()); }}
        onMouseDown={(e) => {
          const el = e.currentTarget;
          const t = window.setTimeout(() => setOpen(el.getBoundingClientRect()), 320);
          const up = () => { window.clearTimeout(t); window.removeEventListener('mouseup', up); };
          window.addEventListener('mouseup', up);
        }}
      >
        <Icon name={cur.icon} size={20} />
        <i className="tool-more" />
      </button>
      {open && (
        <div className="tool-flyout" style={{ left: Math.min(horizontal ? open.left : open.right + 2, window.innerWidth - 240), top: horizontal ? open.bottom + 2 : Math.max(0, open.top - 60) }}>
          {MODES.map((m) => (
            <button key={m.id} className={`${m.id === mode ? 'on' : ''}${m.working ? '' : ' off'}`} disabled={!m.working} title={m.working ? undefined : m.label} onClick={() => { onMode(m.id); setOpen(null); }} {...help(MODE_HELP[m.id], '.tool-flyout')}>
              <span className="fly-dot" aria-hidden="true" />
              <Icon name={m.icon} size={16} />
              <span className="fly-label">{m.label}</span>
              <span className="fly-key">{m.key ?? ''}</span>
            </button>
          ))}
        </div>
      )}
    </>
  );
}
