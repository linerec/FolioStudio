import { useSyncExternalStore } from 'react';
import { ko } from './ko';
import { en } from './en';
import messages from './messages.en';
import { shortcutText } from '../ui/keys';

export type Language = 'ko' | 'en';
export type LanguageChoice = Language | 'system';
const listeners = new Set<() => void>();
const osLanguage = () => typeof navigator !== 'undefined' && /^ko(?:-|$)/i.test(navigator.language) ? 'ko' : 'en';
let language: Language = osLanguage();
export const currentLanguage = () => language;
export function setLanguage(choice: LanguageChoice, systemLanguage?: string) {
  const next = choice === 'ko' || choice === 'en' ? choice : systemLanguage ? (/^ko(?:-|$)/i.test(systemLanguage) ? 'ko' : 'en') : osLanguage();
  if (typeof document !== 'undefined') document.documentElement.lang = next;
  if (next === language) return;
  language = next;
  listeners.forEach(fn => fn());
}
const subscribe = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
export const useLanguage = () => useSyncExternalStore(subscribe, currentLanguage);
const sourceText = (key: string) => Object.hasOwn(ko, key) ? (ko as Record<string, string>)[key] : key;
export const hasTranslation = (source: string) => Object.hasOwn(en, source) || Object.hasOwn(en, sourceText(source));

// Undo history can outlive a language switch. Only known application action labels
// are translated here; document names, style names and user text never enter this lookup.
const actionKeys = Object.keys(ko).filter(k => k.startsWith('action.'));
export function actionLabel(label: string): string {
  const source = ko as Record<string, string>, english = en as Record<string, string>;
  const key = actionKeys.find(k => source[k] === label || english[k] === label);
  const message = Object.entries(messages).find(([ko, en]) => ko === label || en === label);
  return key ? tr(key) : message ? tr(message[0]) : label;
}

/** Korean source is the key and the fallback. IDs remain for generated menus/structured metadata. */
export function tr(source: string, values: Record<string, string | number | boolean | null | undefined> = {}): string {
  const korean = sourceText(source), english = en as Record<string, string>;
  const translated = Object.hasOwn(english, source) ? english[source]
    : Object.hasOwn(english, korean) ? english[korean] : korean;
  const text = shortcutText(language === 'ko' ? korean : translated);
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => Object.hasOwn(values, name) ? String(values[name] === false ? '' : values[name] ?? '') : whole);
}
// Generated menus and existing imports may keep the short alias.
export const t = tr;
/** Calls `fn` whenever the language changes (the preload follows it: engine-locale.ts). */
export const subscribeLanguage = subscribe;
