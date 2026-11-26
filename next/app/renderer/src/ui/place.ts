import { t as tr } from '../i18n';
import { engineMessage } from '../i18n/engine-message';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { EditCommand, EditResult } from '../engine';
import { getPlaceInfo, getTextImportInfo, type PdfOptions } from '../engine';
import { usePdfOptions } from './pdfoptions';
import { importKind, useImportOptions, type ImportOptions } from './importoptions';

export type PlaceTarget = { id: string } | { page: string; point: [number, number] } | { page: string; rect: [number, number, number, number] };
export type Placement = ReturnType<typeof usePlacement>;

/** A text file (placed as text into a text frame), not a picture. */
export const isTextFile = (file: string) => /\.(txt|docx|rtf)$/i.test(file);

/** What a placed file may land on with one click: a text file on a text frame or an empty shape, a picture on a shape. */
export const isPlaceTarget = (file: string | undefined, kind: string) =>
  !!file && (isTextFile(file) ? kind === 'textFrame' || ['rectangle', 'oval', 'polygon'].includes(kind) : ['rectangle', 'oval', 'polygon'].includes(kind));

// Shift held when Place was asked for (a click on the menu item, or the keys) shows the import options, as InDesign's Shift + Open does.
let shiftAtAsk = false;
if (typeof window !== 'undefined') {
  const note = (e: KeyboardEvent | PointerEvent) => { shiftAtAsk = e.shiftKey; };
  window.addEventListener('pointerdown', note, true);
  window.addEventListener('keydown', note, true);
}
/** Whether Shift was held when the last click or key press came (the one that asked for Place). */
export const shiftHeldAtAsk = () => shiftAtAsk;

const fileName = (file: string) => file.split(/[\\/]/).pop() ?? file;

/** The engine's refusal in the language of the screen (the sentences are the Korean text of the engine; the English is in messages.en.ts). */
export function engineText(message: string): string {
  if (message.startsWith('SVG:')) return tr('이 SVG를 그리지 못했습니다: {detail}', { detail: message.slice(4).trim() });
  const format = /^이 형식은 아직 지원하지 않습니다: (\.[\w]+)\. JPEG·PNG·TIFF·PSD·GIF·BMP·PDF를 놓을 수 있습니다$/.exec(message);
  return format ? tr('이 형식은 아직 지원하지 않습니다: {name}. JPEG·PNG·TIFF·PSD·GIF·BMP·PDF를 놓을 수 있습니다', { name: format[1] }) : tr(message);
}

/** A loaded place cursor. Each successful click is one undoable placement; Esc unloads. */
export function usePlacement(document: string | undefined, editable: boolean,
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>,
  selectTool: () => void, note: (message: string) => void,
  /** The "Show Import Options" setting: kept by the app (user settings), asked again in the dialog. */
  showOptions: { show: boolean; set: (show: boolean) => void } = { show: false, set: () => undefined }) {
  const [files, setFiles] = useState<string[]>([]);
  const pending = useRef<{ file: string; pdf?: PdfOptions; text?: boolean; options?: ImportOptions }[]>([]);
  const options = usePdfOptions(document);
  const importOptions = useImportOptions(document, showOptions);
  const busy = useRef(false);
  const generation = useRef(0);
  const cancel = useCallback(() => { generation.current++; pending.current = []; setFiles([]); importOptions.cancel(); }, [importOptions.cancel]);
  useEffect(cancel, [document, editable, cancel]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && pending.current.length) { cancel(); e.preventDefault(); e.stopImmediatePropagation(); }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [cancel]);
  const place = useCallback(async (target: PlaceTarget) => {
    if (!editable || busy.current || !pending.current.length) return null;
    const source = pending.current[0], token = generation.current;
    busy.current = true;
    try {
      // a text file goes into a text frame or an empty shape (a click on it), a frame drawn by dragging, or the frame a click on the page makes
      const command: EditCommand = source.text
        ? { op: 'placeText', file: source.file, ...(source.options ? { options: source.options } : {}), ...target }
        : { op: 'placeGraphic', file: source.file, ...(source.pdf ? { pdf: source.pdf } : {}), ...(source.options ? { options: source.options } : {}), ...target };
      const result = await edit(tr('가져와 놓기'), [command]);
      // characters the text's font lacks took installed fonts: said in the import report
      if (result?.glyphFonts?.length) {
        note(tr('없는 글자에 설치된 글꼴을 썼습니다: {list}', { list: result.glyphFonts.map((f) => tr('{font} ({count}자: {chars})', { font: `${f.family} ${f.style}`, count: f.count, chars: f.chars })).join(' · ') }));
      }
      // what a Word or RTF file has that was not carried over is told, never dropped silently
      if (result && source.text) {
        try {
          const info = await getTextImportInfo(source.file, source.options);
          if (info.warnings.length) note(tr('가져오지 못한 것: {list}', { list: info.warnings.map((message, i) => engineMessage({ message, ...info.warningMessages?.[i] })).join(' · ') }));
        } catch { /* the text is in; only the list is missing */ }
      }
      if (result && token === generation.current) {
        pending.current = pending.current.slice(1); setFiles(pending.current.map((p) => p.file));
      }
      return result;
    } finally { busy.current = false; }
  }, [editable, edit, note]);
  const begin = useCallback(async (paths: string[], selected?: string, askOptions = showOptions.show) => {
    if (!editable || busy.current || !paths.length) return;
    cancel(); const token = generation.current;
    const entries: { file: string; pdf?: PdfOptions; text?: boolean; options?: ImportOptions }[] = [];
    busy.current = true;
    try {
      for (const file of paths) {
        if (/\.(pdf|ai)$/i.test(file)) {
          const info = await getPlaceInfo(file);
          if (token !== generation.current) return;
          if (!info.pdf) throw new Error(tr('PDF 호환 AI 파일만 놓을 수 있습니다.'));
          const choice = await options.choose(file, info);
          if (!choice || token !== generation.current) return;
          entries.push(...choice.pages.map((pageNumber) => ({ file, pdf: { ...choice.options, pageNumber } })));
        } else {
          // Refuse unsupported formats and SVG features before loading the placement cursor.
          if (!isTextFile(file)) {
            const info = await getPlaceInfo(file);
            if (token !== generation.current) return;
            if (info.unsupported) { note(`${fileName(file)}: ${engineText(info.unsupported)}`); continue; }
          }
          // "Show Import Options": the options of this file (a cancel puts nothing down)
          // without the window a plain text file is read with the character set its bytes look like
          const chosen = askOptions ? await importOptions.choose(file) : importKind(file) === 'txt' ? { characterSet: 'AUTO' } : undefined;
          if (chosen === null || token !== generation.current) return;
          entries.push(isTextFile(file) ? { file, text: true, ...(chosen ? { options: chosen } : {}) } : { file, ...(chosen ? { options: chosen } : {}) });
        }
      }
    } catch (e) { note(String((e as Error).message ?? e)); return; }
    finally { busy.current = false; }
    if (token !== generation.current || !entries.length) return;
    pending.current = entries; setFiles(entries.map((p) => p.file)); selectTool();
    if (selected) await place({ id: selected });
  }, [editable, place, selectTool, cancel, options.choose, importOptions.choose, showOptions.show, note]);
  return { files, begin, place, cancel, dialog: options.dialog ?? importOptions.dialog, showOptions };
}
