// The shared number / combo field. One component for every panel, control bar and dialog box that takes a number, so a
// change here is a change for all of them (README.ko.md next to this file has the properties and how to move a field over).
//
// What it does, as a design tool's field does:
//   typing          Enter, Tab or leaving the field applies; Esc goes back. An entry that is not a number, or is outside
//                   min..max, is refused and the value shown again.
//   ▲▼ buttons      press (and hold: they repeat) to step; the value is shown provisionally while the button is down and
//                   made permanent when it is let go.
//   arrow keys      ↑ ↓ step (Shift 10 times, Alt a tenth); the step is made permanent when the key is let go.
//   wheel           over the field with focus, a turn steps; made permanent a moment after the last turn.
//   scrubbing       drag the label (`scrubLabel`, or the row's label with `scrubRowLabel`) sideways; provisional while
//                   dragging, permanent on release, Esc takes it back.
//   list            `options` gives a list (the ▾ button, F4): the item under the pointer or the one the arrow keys
//                   stand on is shown provisionally in the document, a click or Enter makes it permanent, Esc, a click
//                   outside or closing the list gives the original value back exactly.
// The field itself knows only three callbacks: onPreview(value), onCommit(value), onCancel(). The panel (usePreviewEdit)
// turns them into the engine's preview session: nothing about editing lives here.
import { type ReactNode, useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../../i18n';
import './numberfield.css';
import {
  type FieldSpec, SCRUB_START_PX, clampValue, commitAction, formatNumber, indexOfValue, listMove, parseNumber, roundValue, scrubValue, stepValue, wheelDirection,
} from './numberfield.logic';

export type NumberOption = number | { value: number; label?: string };

export type NumberFieldProps = {
  /** The value in the field's unit; null is a mixed selection (the field is blank). */
  value: number | null;
  /** Optional session-owned text, including an unfinished number. */
  textValue?: string | null;
  onTextChange?: (text:string|null) => void;
  /** The value is chosen: typed, picked, a step or a drag let go. */
  onCommit: (value: number) => unknown;
  /** The value is being tried: a list item under the pointer, a step held down, a drag. The panel shows it provisionally. */
  /** `base` is the value the field had when this trial began: what a relative edit (move by, scale by) is measured from. */
  onPreview?: (value: number, base: number | null) => void;
  /** The trial is over without a choice; the panel gives the document back. */
  onCancel?: () => void;
  /** The field's name (accessible name and tooltip). */
  label: string;
  /** A tooltip other than the label. */
  title?: string;
  min: number;
  max: number;
  /** One arrow press (default 1). */
  step?: number;
  /** Digits shown (default 3). */
  decimals?: number;
  /** Written after the number, shown and accepted when typed: " pt", "%", "°". */
  suffix?: string;
  /** The list of values (▾ button, F4). */
  options?: NumberOption[];
  disabled?: boolean;
  width?: number | string;
  /** Reads what was typed (units, sums): the number in the field's unit, or null when it is refused. Default: a number with the suffix. */
  parse?: (text: string) => number | null;
  /** Writes a value for the box. Default: the number trimmed to `decimals` digits and the suffix. */
  format?: (value: number) => string;
  /** A label drawn before the box that scrubs it when dragged. */
  scrubLabel?: ReactNode;
  /** The `.p-label` of the enclosing `.p-row` scrubs the field. */
  scrubRowLabel?: boolean;
  /** Any element (a label or icon of the caller's) that scrubs the field. */
  scrubElement?: HTMLElement | null;
  /** Where a mixed (blank) field starts its first step from (default 0 kept inside the range). */
  anchor?: number;
  /** The ▲▼ buttons (default on). */
  spin?: boolean;
  className?: string;
  /** Marks the field for scripts and tests. */
  fieldId?: string;
  /** Shown in a blank (mixed) box. */
  placeholder?: string;
  /** No key typed in the box reaches the window's shortcuts or a text editor listening there. */
  isolateKeys?: boolean;
  /** In a dialog: Enter applies the entry and leaves the key's own action alone, so the form's default button still works; Esc takes
   *  the typing back, keeps the focus and is passed on (the dialog decides what it means). */
  inDialog?: boolean;
  /** A blank entry is a choice of its own: no value (a find spec's glyph ID, a drop cap's lines left alone). Without it a blank entry is refused. */
  onClear?: () => void;
  /** The box takes the focus when it appears (a dialog's first box). */
  autoFocus?: boolean;
  /** Extra `data-*` attributes of the input element (scripts and tests find a box by them). */
  inputData?: Record<`data-${string}`, string>;
  /**
   * A word that is a value of its own, not a number (a leading of "auto"): the blank box shows `text` while `active`, and an entry
   * that is one of `words` (lower case) chooses it (`onChoose`) instead of being read as a number.
   */
  word?: { text: string; words: string[]; active: boolean; onChoose: () => void };
};

const REPEAT_DELAY = 400;
const REPEAT_EVERY = 70;
const WHEEL_COMMIT = 350;

type Mode = 'list' | 'step' | 'spin' | 'scrub' | 'wheel';

export function NumberField(props: NumberFieldProps) {
  const { value, onCommit, label, title, min, max, step = 1, decimals = 3, suffix = '', options, disabled, width = 56, parse, format, scrubLabel, scrubRowLabel, scrubElement, anchor, spin = true, className, fieldId, placeholder, isolateKeys, inDialog, inputData, word, autoFocus, onClear } = props;
  const spec: FieldSpec = { min, max, step, decimals, suffix };
  const fmt = (v: number) => (format ? format(v) : formatNumber(v, spec));
  const shown = value === null ? (word?.active ? word.text : '') : fmt(value);
  const [localText, setLocalText] = useState<string | null>(null);
  const text = props.textValue === undefined ? localText : props.textValue;
  const setText = (next:string|null) => { setLocalText(next); props.onTextChange?.(next); };
  const [trial, setTrial] = useState<number | null>(null);
  const [submitted, setSubmitted] = useState<{value:number}|null>(null);
  const [open, setOpen] = useState(false);
  const [active, setActiveState] = useState(-1);
  const activeRef = useRef(-1);
  // the active item is also kept in a ref: two keys can arrive before React has rendered the first
  const setActive = (i: number) => { activeRef.current = i; setActiveState(i); };
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLSpanElement>(null);
  const handle = useRef<HTMLSpanElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  // what the field is doing, kept in refs: the events that end it come outside React's render
  const mode = useRef<Mode | null>(null);
  const base = useRef<number | null>(null);
  const pending = useRef<number | null>(null);
  const timers = useRef<{ repeat?: number; wheel?: number }>({});
  // Esc leaves the box: the blur that follows is not a commit of what was typed
  const skipBlur = useRef(false);
  const latest = useRef(props);
  latest.current = props;
  useEffect(() => setLocalText(null), [shown]);

  const values = (options ?? []).map((o) => (typeof o === 'number' ? o : o.value));
  const optionLabel = (o: NumberOption) => (typeof o !== 'number' && o.label ? o.label : fmt(typeof o === 'number' ? o : o.value));

  /** Shows `v` provisionally. The first trial of a session remembers the value to give back. */
  const tryValue = useCallback((v: number, kind: Mode) => {
    const p = latest.current;
    const next = clampValue(roundValue(v), p.min, p.max);
    if (mode.current === null || (mode.current === 'list' && pending.current === null)) base.current = p.value;
    mode.current = kind;
    pending.current = next;
    setTrial(next);
    p.onPreview?.(next, base.current);
  }, []);

  /** Ends the session: the value tried is made permanent, or (`keep` false) given back. */
  const finish = useCallback((keep: boolean) => {
    window.clearTimeout(timers.current.repeat);
    window.clearInterval(timers.current.repeat);
    window.clearTimeout(timers.current.wheel);
    const tried = pending.current;
    const was = base.current;
    mode.current = null;
    pending.current = null;
    base.current = null;
    setTrial(null);
    setOpen(false);
    activeRef.current = -1;
    setActiveState(-1);
    if (tried === null) return;
    if (keep && commitAction(tried, was) === 'commit') latest.current.onCommit(tried);
    else latest.current.onCancel?.();
  }, []);

  const typed = () => {
    if (text !== null && text !== shown) {
      if (word?.words.includes(text.trim().toLowerCase())) {
        if (!word.active) word.onChoose();
      } else if (onClear && text.trim() === '') {
        if (value !== null) onClear();
      } else {
        const v = parse ? parse(text) : parseNumber(text, spec);
        if (v !== null && v >= min && v <= max) {
          const result = onCommit(v);
          // Keep a typed value visible until its owner has read back the edit. A failed edit
          // also settles the promise, so the authoritative value is shown again.
          if (result && typeof (result as PromiseLike<unknown>).then === 'function') {
            const entry = {value:v};
            setSubmitted(entry);
            const settled = () => setSubmitted(current => current === entry ? null : current);
            void Promise.resolve(result).then(settled, settled);
          }
        }
      }
    }
    setText(null);
  };

  const modOf = (e: { shiftKey: boolean; altKey: boolean }) => ({ shift: e.shiftKey, alt: e.altKey });
  const current = () => pending.current ?? value;

  // --- the list ---------------------------------------------------------------------------------------------------
  const [where, setWhere] = useState<{ left: number; top?: number; bottom?: number; width: number } | null>(null);
  const openList = () => {
    if (disabled || !options?.length) return;
    input.current?.focus();
    const r = (box.current ?? input.current)!.getBoundingClientRect();
    // below the box, or above it when the box is at the bottom of the window (the status bar's zoom box)
    const need = Math.min((options?.length ?? 0) * 24, 300) + 8;
    const below = window.innerHeight - r.bottom >= need || window.innerHeight - r.bottom >= r.top;
    setWhere(below ? { left: r.left, top: r.bottom, width: Math.max(r.width, 72) } : { left: r.left, bottom: window.innerHeight - r.top, width: Math.max(r.width, 72) });
    mode.current = 'list';
    setActive(indexOfValue(values, value));
    setOpen(true);
  };
  const hover = (i: number) => {
    if (i === activeRef.current || i < 0 || i >= values.length) return;
    setActive(i);
    tryValue(values[i], 'list');
  };
  const pick = (i: number) => {
    if (i < 0 || i >= values.length) return;
    if (pending.current === null || Math.abs((pending.current ?? 0) - values[i]) > 1e-9) tryValue(values[i], 'list');
    finish(true);
  };
  useLayoutEffect(() => {
    if (!open) return;
    // outside the list: a click, a scroll or a resize gives the value back
    const away = (e: Event) => {
      const t = e.target as Node | null;
      if (t && (popup.current?.contains(t) || box.current?.contains(t))) return;
      finish(false);
    };
    const close = () => finish(false);
    document.addEventListener('pointerdown', away, true);
    window.addEventListener('resize', close);
    window.addEventListener('blur', close);
    document.addEventListener('scroll', away, true);
    return () => {
      document.removeEventListener('pointerdown', away, true);
      window.removeEventListener('resize', close);
      window.removeEventListener('blur', close);
      document.removeEventListener('scroll', away, true);
    };
  }, [open, finish]);

  // --- the wheel (a native listener: React's cannot stop the page from scrolling) ----------------------------------
  useEffect(() => {
    const el = input.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const p = latest.current;
      if (p.disabled || document.activeElement !== el || (mode.current !== null && mode.current !== 'wheel')) return;
      const dir = wheelDirection(e.deltaY);
      if (!dir) return;
      e.preventDefault();
      const s: FieldSpec = { min: p.min, max: p.max, step: p.step ?? 1, decimals: p.decimals ?? 3, suffix: p.suffix ?? '' };
      tryValue(stepValue(pending.current ?? p.value, dir, s, modOf(e), p.anchor), 'wheel');
      window.clearTimeout(timers.current.wheel);
      timers.current.wheel = window.setTimeout(() => finish(true), WHEEL_COMMIT);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [tryValue, finish]);

  // --- scrubbing ---------------------------------------------------------------------------------------------------
  useEffect(() => {
    const el: HTMLElement | null | undefined = scrubElement ?? (scrubRowLabel ? input.current?.closest('.p-row')?.querySelector<HTMLElement>('.p-label') : handle.current);
    if (!el) return;
    const onDown = (e: PointerEvent) => {
      const p = latest.current;
      if (p.disabled || e.button !== 0 || mode.current !== null) return;
      e.preventDefault();
      const x0 = e.clientX, start = p.value;
      let moved = false;
      const move = (m: PointerEvent) => {
        const dx = m.clientX - x0;
        if (!moved && Math.abs(dx) < SCRUB_START_PX) return;
        moved = true;
        const s: FieldSpec = { min: p.min, max: p.max, step: p.step ?? 1, decimals: p.decimals ?? 3, suffix: p.suffix ?? '' };
        tryValue(scrubValue(start, dx, s, modOf(m), p.anchor), 'scrub');
      };
      const stop = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        window.removeEventListener('pointercancel', cancel);
        window.removeEventListener('keydown', key, true);
        document.body.classList.remove('nf-scrubbing');
      };
      const up = () => { stop(); if (moved) finish(true); };
      const cancel = () => { stop(); if (moved) finish(false); };
      const key = (k: KeyboardEvent) => {
        if (k.key !== 'Escape') return;
        k.preventDefault();
        k.stopPropagation();
        cancel();
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
      window.addEventListener('pointercancel', cancel);
      window.addEventListener('keydown', key, true);
      document.body.classList.add('nf-scrubbing');
    };
    el.addEventListener('pointerdown', onDown);
    el.classList.add('nf-scrub');
    return () => { el.removeEventListener('pointerdown', onDown); el.classList.remove('nf-scrub'); };
  }, [scrubRowLabel, scrubLabel, scrubElement, tryValue, finish]);

  // --- the ▲▼ buttons ----------------------------------------------------------------------------------------------
  const spinDown = (dir: 1 | -1) => (e: React.PointerEvent) => {
    if (disabled || e.button !== 0 || (mode.current !== null && mode.current !== 'spin')) return;
    e.preventDefault();
    input.current?.focus();
    const m = modOf(e);
    tryValue(stepValue(current(), dir, spec, m, anchor), 'spin');
    timers.current.repeat = window.setTimeout(() => {
      timers.current.repeat = window.setInterval(() => tryValue(stepValue(pending.current ?? latest.current.value, dir, spec, m, anchor), 'spin'), REPEAT_EVERY);
    }, REPEAT_DELAY);
  };
  const spinUp = (e: React.PointerEvent) => {
    if (mode.current !== 'spin') return;
    e.preventDefault();
    finish(true);
  };

  // --- the box -----------------------------------------------------------------------------------------------------
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (isolateKeys) e.stopPropagation();
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Enter') {
      if (!inDialog || open) e.preventDefault();
      if (open && activeRef.current >= 0) pick(activeRef.current); else if (mode.current === 'step' || mode.current === 'wheel') finish(true); else typed();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (mode.current !== null) { e.stopPropagation(); finish(false); } else { skipBlur.current = true; setText(null); if (!inDialog) e.currentTarget.blur(); skipBlur.current = false; }
    } else if (e.key === 'F4' && options?.length) {
      e.preventDefault();
      if (open) finish(false); else openList();
    } else if (open && ['ArrowDown', 'ArrowUp', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
      e.preventDefault();
      e.stopPropagation();
      const i = listMove(activeRef.current, e.key, values.length);
      setActive(i);
      if (i >= 0) tryValue(values[i], 'list');
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();
      if (disabled || (mode.current !== null && mode.current !== 'step')) return;
      tryValue(stepValue(current(), e.key === 'ArrowUp' ? 1 : -1, spec, modOf(e), anchor), 'step');
    }
  };
  const onKeyUp = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && mode.current === 'step') finish(true);
  };
  const onBlur = () => {
    if (skipBlur.current) return;
    if (mode.current === 'list') finish(false);
    else if (mode.current !== null) finish(true);
    else typed();
  };

  const listId = `${id}-list`;
  // a value being tried is shown at once (the box's own text is stale while a step, a wheel turn or a drag is under way)
  const display = trial !== null ? fmt(trial) : (text ?? (submitted ? fmt(submitted.value) : shown));
  return (
    <span className={`nf${className ? ` ${className}` : ''}${disabled ? ' disabled' : ''}`} data-field={fieldId} data-trial={trial !== null ? '1' : undefined}>
      {scrubLabel !== undefined && <span ref={handle} className="nf-label">{scrubLabel}</span>}
      <span ref={box} className="nf-box" style={{ width }}>
        <input ref={input} className="p-input nf-input" aria-label={label} title={title ?? label} value={display} disabled={disabled} placeholder={placeholder} autoFocus={autoFocus} {...inputData}
          {...(options?.length ? { role: 'combobox', 'aria-expanded': open, 'aria-controls': open ? listId : undefined, 'aria-haspopup': 'listbox' as const, 'aria-autocomplete': 'none' as const,
            'aria-activedescendant': open && active >= 0 ? `${id}-o${active}` : undefined } : {})}
          onFocus={(e) => { setText(shown); e.currentTarget.select(); }}
          onChange={(e) => setText(e.target.value)}
          onBlur={onBlur} onKeyDown={onKeyDown} onKeyUp={onKeyUp} />
        {(spin || options?.length) && !disabled && (
          <span className="nf-buttons" onMouseDown={(e) => e.preventDefault()}>
            {spin && <span className="nf-spin">
              <button type="button" tabIndex={-1} className="nf-up" aria-label={tr('값 늘리기')} title={tr('값 늘리기')}
                onPointerDown={spinDown(1)} onPointerUp={spinUp} onPointerLeave={spinUp} onPointerCancel={spinUp} />
              <button type="button" tabIndex={-1} className="nf-down" aria-label={tr('값 줄이기')} title={tr('값 줄이기')}
                onPointerDown={spinDown(-1)} onPointerUp={spinUp} onPointerLeave={spinUp} onPointerCancel={spinUp} />
            </span>}
            {options?.length ? <button type="button" tabIndex={-1} className="nf-drop" aria-label={tr('값 목록 열기')} title={tr('값 목록 열기')} aria-haspopup="listbox" aria-expanded={open}
              onClick={() => (open ? finish(false) : openList())} /> : null}
          </span>
        )}
      </span>
      {open && where && options && createPortal(
        <div ref={popup} id={listId} className="nf-list" role="listbox" aria-label={label} style={{ left: where.left, top: where.top, bottom: where.bottom, minWidth: where.width }}
          onMouseDown={(e) => e.preventDefault()}>
          {options.map((o, i) => (
            <div key={i} id={`${id}-o${i}`} role="option" aria-selected={i === active} data-value={values[i]} className={`nf-item${i === active ? ' on' : ''}`}
              onPointerEnter={() => hover(i)} onPointerMove={() => hover(i)} onClick={() => pick(i)}>
              {optionLabel(o)}
            </div>
          ))}
        </div>, document.body)}
    </span>
  );
}
