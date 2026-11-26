import { tr } from '../../i18n';

// Hyperlink ▸ Go To Destination. The engine resolves the link to a structured target (query `hyperlinkTarget`: text, page, url) and this
// file applies it to the screen: scroll, zoom, selection, or the system browser. Nothing here decides what the target is.

export type TextTarget = { kind: 'text'; story: string; start: number; end: number };
export type PageView = { setting: 'fixed' | 'fitView' | 'fitWidth' | 'fitHeight' | 'fitVisible' | 'inheritZoom'; percent: number };
export type PageTarget = { kind: 'page'; page: string; view: PageView };
export type UrlTarget = { kind: 'url'; url: string };
export type GotoTarget = TextTarget | PageTarget | UrlTarget;

const SETTINGS: readonly string[] = ['fixed', 'fitView', 'fitWidth', 'fitHeight', 'fitVisible', 'inheritZoom'];

/** The usable target in a `hyperlinkTarget` reply, or undefined (missing link, missing destination, a kind the screen cannot go to). */
export function parseTarget(value: unknown): GotoTarget | undefined {
  const t = value as { kind?: string; story?: unknown; start?: unknown; end?: unknown; page?: unknown; view?: { setting?: unknown; percent?: unknown }; url?: unknown } | undefined;
  if (!t) return undefined;
  if (t.kind === 'text') {
    return typeof t.story === 'string' && Number.isInteger(t.start) && (t.start as number) >= 0 && t.end === t.start
      ? { kind: 'text', story: t.story, start: t.start as number, end: t.start as number } : undefined;
  }
  if (t.kind === 'page') {
    const view = t.view;
    return typeof t.page === 'string' && typeof view?.setting === 'string' && SETTINGS.includes(view.setting) && typeof view.percent === 'number'
      ? { kind: 'page', page: t.page, view: { setting: view.setting as PageView['setting'], percent: view.percent } } : undefined;
  }
  if (t.kind === 'url') return typeof t.url === 'string' ? { kind: 'url', url: t.url } : undefined;
  return undefined;
}

/** What the screen offers for going somewhere: the document view and the selection, and the system browser. */
export type GotoHost = {
  pages: { id?: string; index: number }[];
  viewport: { zoomTo(scale: number): void; fit(what: 'page' | 'width', page: number): void; scrollToPage(page: number, smooth?: boolean): void };
  setCurrent(page: number): void;
  /** Leaves no object selected (the source was an object and the destination a page). */
  clearSelection(): void;
  revealText(to: { story: string; start: number; end: number }): void;
  openExternal(url: string): Promise<{ ok: boolean; reason?: string }>;
  note(text: string): void;
};

/** Where the link started: the selected text of a source, or a selected source object. InDesign (2025, window, measured 2026-10-28): the page destination leaves a text
 *  selection alone and clears the selection of an object source; a text destination selects its insertion point either way. */
export type GotoFrom = 'text' | 'object';

const afterLayout = (then: () => void) => requestAnimationFrame(() => requestAnimationFrame(then));

export function goToTarget(target: GotoTarget, host: GotoHost, from: GotoFrom): void {
  if (target.kind === 'text') { host.revealText(target); return; }
  if (target.kind === 'url') {
    void host.openExternal(target.url).then((r) => {
      if (r.ok) return;
      host.note(r.reason === 'empty' ? tr('링크 대상 주소가 비어 있습니다.')
        : r.reason === 'scheme' || r.reason === 'relative' ? tr('이 주소는 열 수 없습니다. 웹(http·https)과 메일(mailto) 주소만 열 수 있습니다: {url}', { url: target.url })
        : tr('주소를 열지 못했습니다: {url}', { url: target.url }));
    }, () => host.note(tr('주소를 열지 못했습니다: {url}', { url: target.url })));
    return;
  }
  const page = host.pages.find((p) => p.id === target.page);
  if (!page) { host.note(tr('대상 쪽을 찾을 수 없습니다.')); return; }
  const { setting, percent } = target.view;
  host.setCurrent(page.index);
  // Fixed N is N percent of actual size. Fit View, Fit Height and Fit Visible show the whole page (InDesign gave the same zoom for the three on a portrait page);
  // Fit Width fits the width; Inherit Zoom keeps the zoom. Where the page sits in the window is not measured: it is centred (the top for Fit Width).
  if (setting === 'fixed') host.viewport.zoomTo(percent / 100);
  else if (setting === 'fitWidth') host.viewport.fit('width', page.index);
  else if (setting !== 'inheritZoom') host.viewport.fit('page', page.index);
  if (setting === 'fixed' || setting === 'inheritZoom') afterLayout(() => host.viewport.scrollToPage(page.index, false));
  if (from === 'object') host.clearSelection();
}
