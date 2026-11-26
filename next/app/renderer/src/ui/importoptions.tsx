import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { engineMessage } from '../i18n/engine-message';
import { getTextImportInfo, type TextImportInfo } from '../engine';

/** What the dialog asks about: a plain text file, a Word or RTF file, or a picture. */
export type ImportKind = 'txt' | 'word' | 'picture';
/** The import options of one file as the engine takes them (`placeText.options`, `placeGraphic.options`). */
export type ImportOptions = Record<string, unknown>;
type Request = { file: string; kind: ImportKind; done: (options: ImportOptions | null) => void };

export const importKind = (file: string): ImportKind => /\.txt$/i.test(file) ? 'txt' : /\.(docx|rtf)$/i.test(file) ? 'word' : 'picture';

const characterSets = (): [string, string][] => [
  ['AUTO', tr('자동 감지')],
  ['ANSI', tr('ANSI (윈도우 서유럽)')], ['UTF8', 'UTF-8'], ['UTF16', 'UTF-16'], ['KSC5601', tr('한국어 (KS C 5601)')],
  ['SHIFTJIS90MS', tr('일본어 (Shift JIS)')], ['GB2312', tr('중국어 간체 (GB 2312)')], ['GB18030', 'GB 18030'],
  ['CHINESE_BIG_5', tr('중국어 번체 (Big 5)')], ['MACINTOSH_ROMAN', tr('맥 로마자')], ['WINDOWS_CYRILLIC', tr('윈도우 키릴 문자')],
  ['WINDOWS_EE', tr('윈도우 중앙 유럽')], ['WINDOWS_GREEK', tr('윈도우 그리스 문자')], ['WINDOWS_TURKISH', tr('윈도우 터키어')],
  ['WINDOWS_BALTIC', tr('윈도우 발트어')], ['WINDOWS_ARABIC', tr('윈도우 아랍 문자')], ['WINDOWS_HEBREW', tr('윈도우 히브리 문자')],
  ['CYRILLIC_KOI8R', 'KOI8-R'], ['CYRILLIC_KOI8U', 'KOI8-U'], ['CYRILLIC_ISO', 'ISO 8859-5'], ['GREEK_ISO', 'ISO 8859-7'],
  ['CENTRALEUROPEAN_ISO', 'ISO 8859-2'], ['TURKISH_ISO', 'ISO 8859-9'],
];
const characterSetName = (value: string | null | undefined) => characterSets().find(([v]) => v === value)?.[1] ?? value ?? '';
const intents = (): [string, string][] => [
  ['UseColorSettings', tr('색상 설정 따르기')], ['Perceptual', tr('지각')], ['Saturation', tr('채도')],
  ['RelativeColorimetric', tr('상대 색도계')], ['AbsoluteColorimetric', tr('절대 색도계')],
];

function ImportOptionsDialog({ request, remember }: { request: Request; remember: { show: boolean; set: (show: boolean) => void } }) {
  const { kind } = request;
  const [characterSet, setCharacterSet] = useState('AUTO');
  const [quotes, setQuotes] = useState(true);
  const [stripLines, setStripLines] = useState(false);
  const [stripParagraphs, setStripParagraphs] = useState(false);
  const [spacesToTabs, setSpacesToTabs] = useState(false);
  const [spaces, setSpaces] = useState(3);
  const [removeFormatting, setRemoveFormatting] = useState(false);
  const [pageBreaks, setPageBreaks] = useState('PAGE_BREAK');
  const [graphics, setGraphics] = useState(true);
  const [fillGlyphs, setFillGlyphs] = useState(true);
  const [intent, setIntent] = useState('UseColorSettings');
  const [show, setShow] = useState(remember.show);
  const [info, setInfo] = useState<TextImportInfo | string | null>(null);
  const options = (): ImportOptions => kind === 'picture' ? { renderingIntent: intent } : {
    ...(kind === 'txt' ? { characterSet } : {}),
    useTypographersQuotes: quotes, stripReturnsBetweenLines: stripLines, stripReturnsBetweenParagraphs: stripParagraphs,
    convertSpacesIntoTabs: spacesToTabs, spacesIntoTabsCount: spaces,
    ...(kind === 'word' ? { removeFormatting, convertPageBreaks: pageBreaks, preserveGraphics: graphics } : {}),
    // Folio: characters the text's font lacks take an installed font that has them (on by default)
    ...(fillGlyphs ? {} : { fillMissingGlyphs: false }),
  };
  // what the file becomes with these options (the engine reads the file, nothing is placed)
  const key = JSON.stringify(options());
  useEffect(() => {
    if (kind === 'picture') return;
    let live = true;
    setInfo(null);
    const timer = setTimeout(() => {
      getTextImportInfo(request.file, JSON.parse(key) as ImportOptions).then((i) => live && setInfo(i)).catch((e) => live && setInfo(String((e as Error).message ?? e)));
    }, 150);
    return () => { live = false; clearTimeout(timer); };
  }, [key, kind, request.file]);
  const title = tr('가져오기 옵션');
  const box = (label: string, value: boolean, set: (v: boolean) => void) => <label style={{ justifyContent: 'flex-start', gap: 8 }}><input type="checkbox" checked={value} onChange={(e) => set(e.target.checked)} />{label}</label>;
  return createPortal(<div className="style-modal" role="dialog" aria-label={title}
    onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') request.done(null); }}>
    <form className="card dialog style-options" style={{ width: 440 }} onSubmit={(e) => { e.preventDefault(); remember.set(show); request.done(options()); }}>
      <h2>{title}</h2>
      <p style={{ overflowWrap: 'anywhere' }}>{request.file.split(/[/\\]/).pop()}</p>
      {kind === 'picture' && <label>{tr('렌더링 의도')}<select autoFocus aria-label={tr('렌더링 의도')} value={intent} onChange={(e) => setIntent(e.target.value)}>
        {intents().map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>}
      {kind === 'txt' && <label>{tr('문자 집합')}<select autoFocus aria-label={tr('문자 집합')} value={characterSet} onChange={(e) => setCharacterSet(e.target.value)}>
        {characterSets().map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>}
      {kind === 'word' && box(tr('글에서 스타일과 서식 제거'), removeFormatting, setRemoveFormatting)}
      {kind === 'word' && <label>{tr('쪽 나누기')}<select aria-label={tr('쪽 나누기')} value={pageBreaks} onChange={(e) => setPageBreaks(e.target.value)}>
        <option value="PAGE_BREAK">{tr('쪽 나누기 유지')}</option><option value="COLUMN_BREAK">{tr('단 나누기로 바꾸기')}</option><option value="NONE">{tr('없애기')}</option>
      </select></label>}
      {kind === 'word' && box(tr('그림 가져오기'), graphics, setGraphics)}
      {kind !== 'picture' && box(tr('둥근 따옴표 쓰기'), quotes, setQuotes)}
      {kind !== 'picture' && box(tr('없는 글자에 설치된 글꼴 쓰기'), fillGlyphs, setFillGlyphs)}
      {kind === 'txt' && box(tr('줄 끝의 줄바꿈 없애기'), stripLines, setStripLines)}
      {kind === 'txt' && box(tr('문단 끝의 줄바꿈 없애기'), stripParagraphs, setStripParagraphs)}
      {kind === 'txt' && box(tr('공백을 탭으로 바꾸기'), spacesToTabs, setSpacesToTabs)}
      {kind === 'txt' && spacesToTabs && <label>{tr('공백 개수')}<input type="number" min={2} max={99} aria-label={tr('공백 개수')} value={spaces} onChange={(e) => setSpaces(Math.max(2, Math.min(99, Math.round(Number(e.target.value)) || 3)))} /></label>}
      {kind !== 'picture' && <p role="status" aria-live="polite">{info === null ? tr('읽는 중…') : typeof info === 'string' ? info
        : tr('{paragraphs}문단 · {characters}자', { paragraphs: info.paragraphs, characters: info.characters })
          + (kind === 'txt' && characterSet === 'AUTO' && info.characterSet ? ' · ' + tr('찾은 문자 집합: {name}', { name: characterSetName(info.characterSet) }) : '')}</p>}
      {kind === 'txt' && typeof info === 'object' && info && <pre aria-label={tr('미리 보기')} style={{ maxHeight: 72, overflow: 'auto', whiteSpace: 'pre-wrap', margin: 0, fontSize: 12 }}>{info.preview.replace(/\r/g, '\n')}</pre>}
      {kind !== 'picture' && typeof info === 'object' && info && info.warnings.length > 0 && <p>{tr('가져오지 못한 것: {list}', { list: info.warnings.map((message, i) => engineMessage({ message, ...info.warningMessages?.[i] })).join(' · ') })}</p>}
      {box(tr('다음에도 가져오기 옵션 표시'), show, setShow)}
      <footer style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}><button type="button" onClick={() => request.done(null)}>{tr('취소')}</button><button type="submit">{tr('확인')}</button></footer>
    </form>
  </div>, document.body);
}

/** The import options dialog of a text file or a picture, asked for in turn: `choose` answers with the options, or null when the person cancels. */
export function useImportOptions(document: string | undefined, remember: { show: boolean; set: (show: boolean) => void }) {
  const [request, setRequest] = useState<Request | null>(null);
  const resolver = useRef<((options: ImportOptions | null) => void) | null>(null);
  const done = useCallback((options: ImportOptions | null) => {
    const resolve = resolver.current; resolver.current = null; setRequest(null); resolve?.(options);
  }, []);
  useEffect(() => () => { resolver.current?.(null); resolver.current = null; }, []);
  useEffect(() => { done(null); }, [document, done]);
  const choose = useCallback((file: string) => new Promise<ImportOptions | null>((resolve) => {
    resolver.current?.(null); resolver.current = resolve; setRequest({ file, kind: importKind(file), done });
  }), [done]);
  // a stable function: usePlacement keeps it in the dependencies of its own cancel
  const cancel = useCallback(() => done(null), [done]);
  return { choose, cancel, dialog: request ? <ImportOptionsDialog key={request.file} request={request} remember={remember} /> : null };
}
