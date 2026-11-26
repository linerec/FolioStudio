// The Links panel's selection and the panel menu commands that need no change to the document:
// Go to Link, Copy Info (full path, platform style path, all links' or the selected link's
// information), Utilities ▸ Search for Missing Links (the search folders again) and Show Link
// Information Pane; and the ones that change the document: Relink (the file picked), Update Link (a modified link takes its file again), Embed Link (oracle basic-links).
import { useRef, useState } from 'react';
import type { EditCommand, EditResult, LinkRow, LinksReport } from '../engine';
import type { Target } from '../print';
import type { Command } from './commands';
import { tr } from '../i18n';
import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';

const INFO_OPEN = 'folio.linksPanel.info';
const AUTO_SIZE = 'folio.linksPanel.autoSize';
const COLUMNS = 'folio.linksPanel.columns';
const THUMBNAILS = 'folio.linksPanel.thumbnails';
/** The columns of the Links panel list besides the name (this computer's setting: Panel Options…). */
export type LinkColumns = { status: boolean; page: boolean };

function linkColumns(value: string | null): LinkColumns {
  try {
    const columns = JSON.parse(value ?? 'null');
    return { status: columns?.status !== false, page: columns?.page !== false };
  } catch { return { status: true, page: true }; }
}

export type LinkMenu = {
  /** The link picked in the panel (its owner's `Self`). */
  selected: LinkRow | null;
  select: (l: LinkRow | null) => void;
  infoOpen: boolean;
  setInfoOpen: (v: boolean) => void;
  /** Utilities ▸ Auto-Size Link Information: the label column of the information pane fits its labels (this computer's setting). */
  autoSize: boolean;
  columns: LinkColumns;
  /** Panel Options… / Link Options… (the columns dialog) is open. */
  optionsOpen: boolean;
  closeOptions: () => void;
  setColumns: (c: LinkColumns) => void;
  /** Panel Options ▸ Show Thumbnail in Name Column (a user setting). */
  thumbnails: boolean;
  setThumbnails: (v: boolean) => void;
  /** The link Relink File Extension… is asking about (its dialog is open), and what the dialog does. */
  extDialog: LinkRow | null;
  closeExt: () => void;
  applyExt: (link: LinkRow, ext: string) => Promise<void>;
  /** Utilities ▸ XMP File Info… / Versions… (the window about the picked link's file is open). */
  fileDialog: { kind: 'xmp' | 'versions'; link: LinkRow } | null;
  closeFileDialog: () => void;
  /** Breaks the picked content link (Unlink): defined when one is picked and the document can be edited. */
  unlinkContent?: () => void;
  /** What the panel's right-click menus (Links Header / Link Info Context Menu) were opened on: the column header, or the information field. */
  setTarget: (t: { column?: string; field?: string }) => void;
  handlers: Record<string, Partial<Command>>;
};

/** The text Copy Info puts on the clipboard: a header line, then one tab-separated line per link. */
export function linkInfoText(rows: LinkRow[]): string {
  const head = ['Name', 'Status', 'Page', 'Path', 'Format', 'Color Space', 'Effective PPI'];
  const line = (l: LinkRow) => [l.name, l.status, l.pageName ?? '', l.path ?? l.recorded, l.format ?? '', l.space ?? '', l.ppi !== null ? String(Math.round(l.ppi)) : ''];
  return [head, ...rows.map(line)].map((r) => r.join('\t')).join('\n');
}

/** The file a link would have with another extension (the same folder, the same name). */
export const withExtension = (p: string, ext: string) => (/\.[^./\\]+$/.test(p) ? p.replace(/\.[^./\\]+$/, '.' + ext) : p + '.' + ext);

/** A path with this computer's separators (Windows: backslashes). */
function platformPath(p: string): string {
  return /^win/i.test(navigator.platform) ? p.replace(/\//g, '\\') : p.replace(/\\/g, '/');
}

export function useLinkMenu({ links, busy, go, relink, note, editable, edit }: {
  links: LinksReport | null; busy: boolean; go: (t: Target) => void; relink: () => void; note: (text: string) => void;
  editable: boolean; edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
}): LinkMenu {
  const [picked, setPicked] = useState<string | null>(null);
  const [extFor, setExtFor] = useState<LinkRow | null>(null);
  const [fileFor, setFileFor] = useState<{ kind: 'xmp' | 'versions'; link: LinkRow } | null>(null);
  const infoOpen = useUserPreference(INFO_OPEN) !== '0';
  const setInfoOpen = (v: boolean) => userSettings.setItem(INFO_OPEN, v ? '1' : '0');
  const autoSize = useUserPreference(AUTO_SIZE) === '1';
  const setAutoSize = (v: boolean) => userSettings.setItem(AUTO_SIZE, v ? '1' : '0');
  const columns = linkColumns(useUserPreference(COLUMNS));
  const setColumns = (c: LinkColumns) => userSettings.setItem(COLUMNS, JSON.stringify(c));
  // Panel Options ▸ Show Thumbnail in Name Column: a user setting the engine keeps (`userSettings`), on unless turned off
  const [thumbnails, setThumbnailsState] = useState(() => { try { return userSettings.getItem(THUMBNAILS) !== '0'; } catch { return true; } });
  const setThumbnails = (v: boolean) => { setThumbnailsState(v); try { userSettings.setItem(THUMBNAILS, v ? '1' : '0'); } catch { /* not kept */ } };
  const [optionsOpen, setOptionsOpen] = useState(false);
  const target = useRef<{ column?: string; field?: string }>({});
  // the row again from the latest link check (the same owner); the content links (Content Placer) are rows of their own
  const rows = [...(links?.links ?? []), ...(links?.contentLinks ?? [])];
  const selected = picked ? rows.find((l) => l.owner === picked) ?? null : null;
  // what needs a file (Relink, Embed, XMP…) is for the placed files, not for a content link
  const file = selected && !selected.content ? selected : null;
  const copy = (text: string) => {
    navigator.clipboard.writeText(text).then(() => note(tr('복사했습니다')), () => note(tr('복사하지 못했습니다')));
  };
  const path = file ? file.path ?? file.recorded : null;
  // Relink to Folder…: the links that are missing (and the picked one) take the file of the same name under the folder picked
  // (the name alone decides; of several files of that name the shortest path wins).
  const relinkable = (links?.links ?? []).filter((l) => l.status === 'missing' || l.status === 'ambiguous');
  const relinkToFolder = async () => {
    const folder = await window.folio.chooseFolder(tr('다시 연결할 파일이 있는 폴더'));
    if (!folder) return;
    const byName = new Map<string, string>();
    for (const f of (await window.folio.listFiles(folder)).sort((a, b) => a.length - b.length)) {
      const key = (f.split(/[\\/]/).pop() ?? f).toLowerCase();
      if (!byName.has(key)) byName.set(key, f);
    }
    const want = new Map([...relinkable, ...(file && file.status !== 'embedded' ? [file] : [])].map((l) => [l.owner, l] as const));
    const hits = [...want.values()].flatMap((l) => { const f = [l.recorded, l.name].map((n) => byName.get((n.split(/[\\/]/).pop() ?? n).toLowerCase())).find((x) => x); return f && f !== l.path ? [{ owner: l.owner, file: f }] : []; });
    if (!hits.length) { note(tr('고른 폴더에 이름이 같은 파일이 없습니다')); return; }
    await edit(tr('폴더에 다시 연결'), hits.map((h) => ({ op: 'relinkGraphic' as const, id: h.owner, file: h.file })));
    note(tr('링크 {0}개를 다시 연결했습니다', { 0: hits.length }));
  };
  // Relink File Extension…: the picked link takes the file of the same name, in its folder, with the extension asked for.
  const applyExt = async (link: LinkRow, ext: string) => {
    const file = withExtension(link.path ?? link.recorded, ext);
    if (!(await window.folio.pathExists(file))) { note(tr('같은 이름에 그 확장자인 파일이 없습니다')); return; }
    await edit(tr('파일 확장자 다시 연결'), [{ op: 'relinkGraphic', id: link.owner, file }]);
    note(tr('링크를 다시 연결했습니다'));
  };
  // Update Link for content links: the placed object (story) takes the source again. One that was edited after the link was made
  // loses its changes, so the question comes first (the link's own option: the engine asks for `overwrite`).
  const updateContent = async (rowsToUpdate: LinkRow[]) => {
    const edited = rowsToUpdate.filter((l) => l.content?.edited);
    if (edited.length && !window.confirm(tr('고친 복사본이 있습니다. 업데이트하면 복사본에서 고친 것이 원본 모양으로 바뀝니다. 계속할까요?'))) return;
    const date = new Date().toISOString().slice(0, 19);
    await edit(rowsToUpdate.length > 1 ? tr('모든 링크 업데이트') : tr('링크 업데이트'),
      rowsToUpdate.map((l) => ({ op: 'updateContentLink' as const, link: l.content!.link, date, overwrite: true })));
  };
  // Relink for a content link InDesign made to another document (its `.indd` address was kept as a hint): joined to the project document
  // imported from that file (engine query `xlinkSources`, command `reconnectContentLinks`; the project holding several such documents
  // asks which). The links of that file all follow.
  const reconnectContent = async (row: LinkRow) => {
    const { header } = await window.folio.request({ cmd: 'xlinkSources' });
    const sources = (header.sources ?? []) as { reference: string; links: string[]; connected: boolean; file: string; candidates: { path: string }[] }[];
    const src = sources.find((x) => !x.connected && x.links.includes(row.content!.link));
    if (!src) { note(tr('이 링크는 프로젝트의 문서에 이을 수 없습니다')); return; }
    if (!src.candidates.length) { note(tr('이 프로젝트에 {0}에서 가져온 문서가 없습니다. 그 문서를 먼저 프로젝트로 가져오세요', { 0: src.file })); return; }
    let target: string | undefined;
    if (src.candidates.length > 1) {
      const list = src.candidates.map((c, i) => `${i + 1}. ${c.path}`).join('\n');
      const picked = Number(window.prompt(tr('{0}에서 가져온 문서가 여러 개입니다. 이을 문서의 번호를 고르세요:\n{1}', { 0: src.file, 1: list }), '1'));
      target = src.candidates[picked - 1]?.path;
      if (!target) return;
    }
    const done = await edit(tr('프로젝트 문서에 다시 연결'), [{ op: 'reconnectContentLinks', document: src.reference, target, to: undefined }]);
    if (done) note(tr('링크 {0}개를 프로젝트의 문서에 이었습니다', { 0: src.links.length }));
  };
  const unlinkContent = editable && selected?.content ? () => {
    void edit(tr('링크 끊기'), [{ op: 'unlinkContentLink', link: selected.content!.link }]);
    setPicked(null);
  } : undefined;
  const handlers: Record<string, Partial<Command>> = {
    'id.panel.links.goToLink': { run: selected && selected.page !== null ? () => go({ page: selected.page!, bounds: selected.bounds }) : undefined },
    'id.panel.links.copyInfo.copyFullPath': { run: path ? () => copy(path) : undefined },
    'id.panel.links.copyInfo.copyPlatformStylePath': { run: path ? () => copy(platformPath(path)) : undefined },
    'id.panel.links.copyInfo.copyAllLinkInformation': { run: rows.length ? () => copy(linkInfoText(rows)) : undefined },
    'id.panel.links.copyInfo.copyInfoForSelectedLinks': { run: selected ? () => copy(linkInfoText([selected])) : undefined },
    // a graphic of the page can be linked again to any file; a link whose file changed takes it again; a found file can be embedded
    'id.panel.links.relink': { run: editable && file && file.status !== 'embedded' ? async () => {
      const [chosen] = await window.folio.placeDialog();
      if (chosen) await edit(tr('다시 연결'), [{ op: 'relinkGraphic', id: file.owner, file: chosen }]);
    } : editable && selected?.content?.external && selected.content.status === 'external' ? () => void reconnectContent(selected) : undefined },
    'id.panel.links.relinkToFolder': { run: editable && (relinkable.length || (file && file.status !== 'embedded')) ? () => void relinkToFolder() : undefined },
    'id.panel.links.relinkFileExtension': { run: editable && file && file.status !== 'embedded' ? () => setExtFor(file) : undefined },
    // a placed file takes its file again; a content link whose source changed takes the source again (an edited copy asks first)
    'id.panel.links.updateLink': { run: editable && file?.status === 'modified' && file.path ? () => void edit(tr('링크 업데이트'), [{ op: 'relinkGraphic', id: file.owner, file: file.path! }])
      : editable && selected?.content?.status === 'outOfDate' ? () => void updateContent([selected]) : undefined },
    'id.panel.links.updateAllLinks': (() => {
      const stale = (links?.links ?? []).filter((l) => l.status === 'modified' && l.path);
      const staleContent = (links?.contentLinks ?? []).filter((l) => l.content?.status === 'outOfDate');
      return { run: editable && (stale.length || staleContent.length) ? () => void (async () => {
        if (stale.length) await edit(tr('모든 링크 업데이트'), stale.map((l) => ({ op: 'relinkGraphic' as const, id: l.owner, file: l.path! })));
        if (staleContent.length) await updateContent(staleContent);
      })() : undefined };
    })(),
    // one command, as InDesign's menu has it: Embed Link, and Unembed Link once the link is embedded
    'id.panel.links.embedLink': file?.status === 'embedded'
      ? { run: editable ? () => void edit(tr('링크 포함 해제'), [{ op: 'unembedLink', id: file.owner }]) : undefined, label: tr('링크 포함 해제') }
      : { run: editable && file && (file.status === 'found' || file.status === 'modified') && file.path ? () => void edit(tr('링크 포함'), [{ op: 'embedLink', id: file.owner, file: file.path! }]) : undefined },
    'id.panel.links.utilities.searchForMissingLinks': { run: links && !busy && (links.summary.missing > 0 || links.summary.ambiguous > 0) ? relink : undefined },
    // both menu items open the dialog for what the list shows (the columns), kept for this computer
    'id.panel.links.panelOptions': { run: () => setOptionsOpen(true) },
    'id.panel.links.linkOptions': { run: () => setOptionsOpen(true) },
    // Utilities ▸ Copy Link(s) To…: the picked link's file, or every found file when none is picked, copied to a folder
    'id.panel.links.utilities.copyLinkSTo': (() => {
      const files = (file ? [file] : selected ? [] : links?.links ?? []).filter((l) => l.path && (l.status === 'found' || l.status === 'modified')).map((l) => l.path!);
      return { run: files.length ? async () => {
        const dir = await window.folio.chooseFolder(tr('링크 파일을 복사할 폴더'));
        if (dir) note(tr('링크 파일 {0}개를 복사했습니다', { 0: await window.folio.copyFiles(dir, files) }));
      } : undefined };
    })(),
    // read-only windows about the picked link's file (an embedded link has no file to read)
    'id.panel.links.utilities.xmpFileInfo': { run: file && file.status !== 'embedded' ? () => setFileFor({ kind: 'xmp', link: file }) : undefined },
    'id.panel.links.utilities.versions': { run: file && file.status !== 'embedded' ? () => setFileFor({ kind: 'versions', link: file }) : undefined },
    'id.panel.links.utilities.autoSizeLinkInformation': { run: () => setAutoSize(!autoSize), checked: autoSize },
    'id.panel.links.utilities.showLinkInformationPane': { run: () => setInfoOpen(!infoOpen), checked: infoOpen },
    // the Links panel's right-click menus: Hide this Column (the column header that was clicked; the name column stays), Copy Info Field (the information field that was clicked)
    'id.ctx.linksHeader.hideThisColumn': { run: () => {
      const column = target.current.column;
      if (column === 'status' || column === 'page') setColumns({ ...columns, [column]: false }); else note(tr('이름 열은 숨길 수 없습니다'));
    } },
    'id.ctx.linkInfo.copyInfoField': { run: () => { const field = target.current.field; if (field) copy(field); else note(tr('복사할 정보 항목을 가리키고 오른쪽 단추를 누르세요')); } },
  };
  return { selected, select: (l) => setPicked(l?.owner ?? null), infoOpen, setInfoOpen, autoSize, columns, setColumns, thumbnails, setThumbnails, optionsOpen, closeOptions: () => setOptionsOpen(false), extDialog: extFor, closeExt: () => setExtFor(null), applyExt, fileDialog: fileFor, closeFileDialog: () => setFileFor(null), unlinkContent, setTarget: (t) => { target.current = t; }, handlers };
}
