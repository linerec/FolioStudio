import type { Command } from '../../ui/commands';
import { tr } from '../../i18n';
import declaration from '../../../../../shared/dialogs/preferences.json';

/** The categories of Edit > Preferences: the tabs of its dialog on the shared frame (next/shared/dialogs/preferences.json). */
export const preferenceTabs = declaration.tabs as { id: string; label: string; command: string; document?: boolean }[];

/** Each category's menu command opens the engine's Preferences dialog on its tab: in the document's scope when one is open for
 *  editing (its settings are changed as one step), otherwise in the app's (the document's categories are greyed then). */
export function preferenceCommands(editable: boolean, flash: (message: string) => void): Record<string, Partial<Command>> {
  return Object.fromEntries(preferenceTabs.map(c => [c.command, {
    run: !c.document || editable ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'preferences', scope: editable ? 'document' : 'app', tab: c.id })
      .then(({ header }) => { if (!header.ok) flash(tr('환경 설정을 열지 못했습니다: {error}', { error: String(header.error) })); }) : undefined,
  }]));
}
