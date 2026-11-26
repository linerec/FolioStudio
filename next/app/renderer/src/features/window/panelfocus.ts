// Panel focus keys (tools-c2): Ctrl+Alt+` puts the keyboard back in the last field used in a panel (InDesign's "Activate last used field in
// panel"), Ctrl+6 in the first field of the control bar ("Activate Control panel"). The screen only remembers which field had the focus; the
// values the fields set are the panels' own commands.
const PANELS = '.dock-area, .dock-column, .floating, .flyout';
const FIELD = 'input:not([type=hidden]):not([disabled]), select:not([disabled]), textarea:not([disabled])';

let last: HTMLElement | null = null;
let installed = false;

/** Starts remembering the last panel field that had the keyboard (once per window). */
export function rememberPanelFields() {
  if (installed || typeof document === 'undefined') return;
  installed = true;
  document.addEventListener('focusin', (e) => {
    const t = e.target;
    if (t instanceof HTMLElement && t.matches(FIELD) && t.closest(PANELS) && !t.closest('[data-text-editor]')) last = t;
  }, true);
}

const take = (el: HTMLElement | null) => {
  if (!el) return false;
  el.focus();
  if (el instanceof HTMLInputElement && el.type !== 'checkbox' && el.type !== 'radio') el.select();
  return document.activeElement === el;
};

/** Ctrl+Alt+`: the last panel field used (still on screen), else the first field of the first panel shown. */
export function focusLastPanelField(): boolean {
  if (last && last.isConnected && last.offsetParent !== null && take(last)) return true;
  const first = [...document.querySelectorAll<HTMLElement>(PANELS)].flatMap((p) => [...p.querySelectorAll<HTMLElement>(FIELD)]).find((f) => f.offsetParent !== null);
  return take(first ?? null);
}

/** Ctrl+6: the first field of the control bar. */
export function focusControlPanel(): boolean {
  const first = [...document.querySelectorAll<HTMLElement>(`.control ${FIELD.split(', ').join(', .control ')}`)].find((f) => f.offsetParent !== null);
  return take(first ?? null);
}

// remembered from the start: the field used before the key is pressed
rememberPanelFields();
