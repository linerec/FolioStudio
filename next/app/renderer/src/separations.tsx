// Separations preview panel (InDesign: Window ▸ Output ▸ Separations Preview): view mode,
// plates with eye toggles, the ink under the cursor, the ink limit.
import { t as tr } from './i18n';
import { type InkInfo, type InkReading, type InkView, type InksReport } from './engine';
import { NumberField } from './ui/controls/numberfield';

export type SepMode = 'off' | 'separations' | 'inkLimit';

export type SepState = {
  mode: SepMode;
  /** Overprint Preview (InDesign: View ▸ Overprint Preview). */
  overprint: boolean;
  /** Plates hidden in the separations view, by name. */
  hidden: string[];
  /** Total area coverage limit, percent. */
  limit: number;
  /** Panel menu ▸ Show Single Plates in Black (on by default): a single shown plate is drawn
   *  as black ink; off, in its own ink colour. */
  singleBlack: boolean;
  /** Panel menu ▸ Desaturate Black (off by default): the separations view shows the black plate at 80 % (measured in InDesign 2026). */
  desaturate: boolean;
};

export const SEP_DEFAULT: SepState = { mode: 'off', overprint: false, hidden: [], limit: 300, singleBlack: true, desaturate: false };

/** The engine view for the panel state; null = the normal screen view. */
export function inkView(s: SepState, report: InksReport | null): InkView | null {
  if (!report) return null;
  if (s.mode === 'separations') {
    const inks = report.inks.map((k) => k.name).filter((n) => !s.hidden.includes(n));
    return { mode: 'separations', inks, ...(s.singleBlack ? {} : { singlePlatesBlack: false }), ...(s.desaturate ? { desaturateBlack: true } : {}) };
  }
  if (s.mode === 'inkLimit') return { mode: 'inkLimit', limit: s.limit };
  return s.overprint ? { mode: 'overprint' } : null;
}

const PROCESS = ['Cyan', 'Magenta', 'Yellow', 'Black'];
const LABEL: Record<string, string> = { get Cyan() { return tr("사이안"); }, get Magenta() { return tr("마젠타"); }, get Yellow() { return tr("노랑"); }, get Black() { return tr("검정"); } };

export function SeparationsView({
  report,
  state,
  setState,
  reading,
  page,
}: {
  report: InksReport | null;
  state: SepState;
  setState: (s: SepState) => void;
  reading: InkReading | null;
  page: number;
}) {
  const percent = (name: string) => reading?.inks.find((i) => i.name === name)?.percent;
  const shown = (name: string) => !state.hidden.includes(name);
  const toggle = (names: string[]) => {
    const allShown = names.every(shown);
    const hidden = allShown ? [...new Set([...state.hidden, ...names])] : state.hidden.filter((n) => !names.includes(n));
    setState({ ...state, mode: state.mode === 'off' ? 'separations' : state.mode, hidden });
  };
  const solo = (name: string) => {
    if (!report) return;
    setState({ ...state, mode: 'separations', hidden: report.inks.map((k) => k.name).filter((n) => n !== name) });
  };
  const fmt = (v: number | undefined) => (v === undefined ? '' : `${Math.round(v)}%`);
  const process = report?.inks.filter((k) => k.kind === 'process') ?? [];
  const spots = report?.inks.filter((k) => k.kind === 'spot') ?? [];
  const processSum = reading ? PROCESS.reduce((s, n) => s + (percent(n) ?? 0), 0) : undefined;
  const sepOn = state.mode === 'separations';
  const over = reading && reading.total > state.limit;
  const onPage = (k: InkInfo) => k.pages.includes(page);
  return (
    <div className="panel-body sep">
      <div className="panel-head">
        <label className="sep-mode">{tr("보기")}<select value={state.mode} onChange={(e) => setState({ ...state, mode: e.target.value as SepMode })}>
            <option value="off">{tr("끔")}</option>
            <option value="separations">{tr("분판")}</option>
            <option value="inkLimit">{tr("잉크 한계")}</option>
          </select>
        </label>
        {state.mode === 'inkLimit' && (
          <label className="sep-limit">
            <NumberField
              value={state.limit}
              min={100}
              max={400 + 100 * spots.length}
              step={10}
              decimals={2}
              width={64}
              label={tr('잉크 한계')}
              onCommit={(v) => setState({ ...state, limit: v })}
            />
            %
          </label>
        )}
        <label className="sep-op" title={tr("오버프린트 미리보기 (Ctrl+Alt+Shift+Y)")}>
          <input type="checkbox" checked={state.overprint || state.mode !== 'off'} disabled={state.mode !== 'off'} onChange={(e) => setState({ ...state, overprint: e.target.checked })} />{tr("오버프린트")}</label>
      </div>
      {!report && <p className="empty">{tr("잉크를 읽는 중…")}</p>}
      {report && (
        <div className="inks">
          <InkRow
            name="CMYK"
            chip="cmyk"
            on={sepOn && process.every((k) => shown(k.name))}
            dim={!sepOn}
            value={fmt(processSum)}
            onEye={() => toggle(PROCESS)}
            onName={() => setState({ ...state, mode: 'separations', hidden: spots.map((k) => k.name) })}
          />
          {process.map((k) => (
            <InkRow key={k.name} ink={k} name={LABEL[k.name] ?? k.name} on={sepOn && shown(k.name)} dim={!sepOn} value={fmt(percent(k.name))} onEye={() => toggle([k.name])} onName={() => solo(k.name)} />
          ))}
          {spots.map((k) => (
            <InkRow
              key={k.name}
              ink={k}
              name={k.name}
              tag={k.inkType && k.inkType !== 'Normal' ? tr('별색 · {0}', {0:k.inkType}) : tr("별색")}
              on={sepOn && shown(k.name)}
              dim={!sepOn}
              faint={!onPage(k)}
              value={fmt(percent(k.name))}
              onEye={() => toggle([k.name])}
              onName={() => solo(k.name)}
            />
          ))}
          <div className={over ? 'ink-total over' : 'ink-total'}>
            <span>{tr("총 잉크량")}</span>
            <b>{reading ? `${Math.round(reading.total)}%` : '–'}</b>
          </div>
        </div>
      )}
      <div className="sep-notes">
        {state.mode === 'inkLimit' && (
          <p>
            <i className="limit-chip" /> {tr('총 잉크량이 {0}%를 넘는 곳 (진할수록 많이 넘음). 나머지는 회색으로 보입니다.', {0:state.limit})}
          </p>
        )}
        {sepOn && (
          <p className="muted">
            {state.singleBlack ? tr('판 하나만 보이면 검정으로, 여럿이면 잉크 색으로 보입니다.') : tr('판 하나만 보여도 그 잉크 색으로 보입니다.')} {tr('이름을 누르면 그 판만 봅니다.')}
          </p>
        )}
        <p className="muted">{tr("페이지 위에 커서를 올리면 그 자리의 잉크 양이 나옵니다.")}</p>
        {report && report.converted.length > 0 && <p className="muted">{tr('원색으로 바뀌는 별색(잉크 관리자): {0}', {0:report.converted.join(', ')})}</p>}
        {report?.inks.filter((k) => k.aliases.length > 0).map((k) => (
          <p key={k.name} className="muted">{tr('{0} → {1} 판 (별칭)', {0:k.aliases.join(', '),1:k.name})}</p>
        ))}
      </div>
    </div>
  );
}

function InkRow({
  ink,
  name,
  chip,
  tag,
  on,
  dim,
  faint,
  value,
  onEye,
  onName,
}: {
  ink?: InkInfo;
  name: string;
  chip?: 'cmyk';
  tag?: string;
  on: boolean;
  dim: boolean;
  faint?: boolean;
  value: string;
  onEye: () => void;
  onName: () => void;
}) {
  const bg = ink ? `rgb(${ink.rgb.join(',')})` : undefined;
  return (
    <div className={`ink-row${faint ? ' faint' : ''}`} title={faint ? tr("이 쪽에서는 쓰이지 않는 잉크") : undefined}>
      <button className={on ? 'eye on' : dim ? 'eye dim' : 'eye'} onClick={onEye} aria-label={tr('{0} 보기', {0:name})} aria-pressed={on}>
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
          <path d="M1 8s2.6-4.5 7-4.5S15 8 15 8s-2.6 4.5-7 4.5S1 8 1 8z" fill="none" stroke="currentColor" strokeWidth="1.3" />
          {on && <circle cx="8" cy="8" r="2.2" fill="currentColor" />}
        </svg>
      </button>
      <i className={chip === 'cmyk' ? 'chip cmyk' : 'chip'} style={bg ? { background: bg } : undefined} />
      <button className="ink-name" onClick={onName} title={tr('{0} 판만 보기', {0:name})}>
        {name}
        {tag && <em>{tag}</em>}
      </button>
      <span className="ink-value">{value}</span>
    </div>
  );
}
