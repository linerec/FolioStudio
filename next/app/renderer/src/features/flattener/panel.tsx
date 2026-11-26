import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { HIGHLIGHTS, type Highlight } from './state';

// Window ▸ Output ▸ Flattener Preview, in InDesign 2025's order and with its on/off rules (rules/windowmenu-c2.ko.md): Highlight, Auto Refresh
// Highlight with Refresh, Preset, Ignore Spread Overrides, Apply Settings to Print. Everything but Highlight is off while Highlight is None;
// Refresh is off while Auto Refresh is on. The state and these rules are the engine's (`flattenerSettings`); the page is drawn by the engine.
export const FLATTENER_PANELS = {
  flattenerPreview: {
    get title() { return tr('병합 미리 보기'); }, icon: 'separations', width: 301, height: 250,
  },
};

const NAMES: Record<Highlight, () => string> = {
  none: () => tr('없음'),
  complexRegions: () => tr('래스터화된 복잡한 영역'),
  transparent: () => tr('투명 개체'),
  affected: () => tr('영향받는 모든 개체'),
  affectedGraphics: () => tr('영향받는 그래픽'),
  outlinedStrokes: () => tr('윤곽선 처리된 획'),
  outlinedText: () => tr('윤곽선 처리된 텍스트'),
  rasterFill: () => tr('래스터 칠 텍스트와 획'),
  rasterized: () => tr('모든 래스터화된 영역'),
};
const BUILT_IN_LABEL: Record<string, () => string> = {
  '[Low Resolution]': () => tr('[저해상도]'),
  '[Medium Resolution]': () => tr('[중간 해상도]'),
  '[High Resolution]': () => tr('[고해상도]'),
};
const ROW = { display: 'flex', alignItems: 'center', gap: 8, margin: '6px 0' } as const;

export function FlattenerPreviewPanel({ ctx }: { ctx: PanelCtx }) {
  const f = ctx.flattener;
  if (!f) return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  const { state, enabled } = f;
  const presets = [...Object.keys(BUILT_IN_LABEL), ...f.customPresets];
  return (
    <div className="panel-body flattener-panel" style={{ padding: 8 }}>
      <label style={ROW}>
        <span style={{ minWidth: 70 }}>{tr('강조')}</span>
        <select aria-label={tr('강조')} value={state.highlight} disabled={!enabled.highlight} style={{ flex: 1 }} onChange={(e) => void f.setHighlight(e.target.value as Highlight)}>
          <option value="none">{NAMES.none()}</option>
          <hr />
          {HIGHLIGHTS.filter((h) => h !== 'none').map((h) => <option key={h} value={h}>{NAMES[h]()}</option>)}
        </select>
      </label>
      <div style={{ ...ROW, justifyContent: 'space-between' }}>
        <label style={ROW}>
          <input type="checkbox" aria-label={tr('자동 새로 고침 강조')} checked={state.autoRefresh} disabled={!enabled.autoRefresh} onChange={(e) => void f.setAutoRefresh(e.target.checked)} />
          {tr('자동 새로 고침 강조')}
        </label>
        <button disabled={!enabled.refresh} onClick={f.refresh}>{tr('새로 고침')}</button>
      </div>
      <label style={ROW}>
        <span style={{ minWidth: 70 }}>{tr('사전 설정')}</span>
        <select aria-label={tr('사전 설정')} value={state.preset} disabled={!enabled.preset} style={{ flex: 1 }} onChange={(e) => void f.setPreset(e.target.value)}>
          {presets.map((name) => <option key={name} value={name}>{BUILT_IN_LABEL[name]?.() ?? name}</option>)}
        </select>
      </label>
      <label style={ROW}>
        <input type="checkbox" aria-label={tr('스프레드 재정의 무시')} checked={state.ignoreSpreadOverrides} disabled={!enabled.ignoreSpreadOverrides} onChange={(e) => void f.setIgnoreSpreadOverrides(e.target.checked)} />
        {tr('스프레드 재정의 무시')}
      </label>
      <button style={{ width: '100%' }} disabled={!enabled.applySettingsToPrint} onClick={() => void f.applyToPrint()}>{tr('인쇄에 설정 적용')}</button>
      <p className="p-note" data-note="flattener">{tr('"래스터화된 복잡한 영역"과 "래스터 칠 텍스트와 획" 강조는 지원하지 않습니다.')}</p>
    </div>
  );
}
