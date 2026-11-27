import { useEffect, useSyncExternalStore } from 'react';
import { tr } from '../i18n';
import {chipLabel,type Snapshot} from './attachments';

export type Fields = Record<string, unknown>;
export type Entry = {
  kind: string; sessionId: string; turnId: string; seq?: number; text?: string; id?: string;
  toolUseId?: string; name?: string; args?: unknown; content?: unknown; isError?: boolean;
  attachments?: Snapshot[]; afterSeq?: number; block?: number; blockType?: string; questionsOnly?: boolean; destination?: string;
  estimatedUsd?: number; sessionEstimatedUsd?: number; budgetWarning?: boolean;
  outside?:boolean; title?:string; toolCallId?:string; entries?:{content:string;status?:string}[];
  permissionId?: string; tool?: string; risk?: string; allowedDecisions?: string[]; sessionTool?: string;
  error?: string; status?: string; mode?: string; model?: string; provider?: string; connectionId?: string;
  documents?: {document: string; summary: {commands: string[]; changedIds: string[]}}[];
  result?: {document:string}; usage?: {inputTokens?: number; outputTokens?: number; cacheReadTokens?: number; cacheWriteTokens?: number};
};

export async function ai(cmd: string, fields: Fields = {}) {
  const {header} = await window.folio.request({...fields, cmd});
  if (!header.ok) throw new Error(String(header.error ?? header.code ?? cmd));
  return header;
}
type Turn = {sessionId: string; turnId: string};
let active: Turn | null = null;
const listeners = new Set<() => void>();
const subscribe = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
export const useTurn = () => useSyncExternalStore(subscribe, () => active);
export const viewTool = (id: string) => ['selection','direct','hand','zoom','measure'].includes(id);
export const runningLabel = () => tr('AI 작업 중 · 멈춤(Esc)');
export function useTurnEvents(onError: (s: string) => void) {
  useEffect(() => {
    const off = window.folio.onEvent(raw => {
      const e = raw as unknown as Entry & {event: string};
      if (e.event !== 'aiEvent') return;
      if (e.kind === 'user') active = {sessionId:e.sessionId, turnId:e.turnId};
      else if (e.kind === 'end' && active?.turnId === e.turnId) active = null;
      else return;
      listeners.forEach(fn => fn());
    });
    const escape = (e: KeyboardEvent) => {
      if (!active || e.key !== 'Escape' || e.isComposing) return;
      e.preventDefault(); e.stopImmediatePropagation();
      void ai('aiStop',active).catch(err => onError(String(err)));
    };
    window.addEventListener('keydown',escape,true);
    return () => { off(); window.removeEventListener('keydown',escape,true); };
  }, [onError]);
}
export type Reference = {label: string; spec?: Fields};
export type Chip = {label:string; spec:Fields; thumbnail?:string; snapshot?:Snapshot};
export async function contextChip(label:string, spec:Fields): Promise<Chip> {
  const context = await ai('aiContext',spec);
  const png = context.image?.png;
  const snapshot=context as Snapshot;
  return {label:label||chipLabel(snapshot),spec:{...spec,document:context.document},snapshot,
    thumbnail: typeof png === 'string' ? 'data:image/png;base64,' + png : undefined};
}
