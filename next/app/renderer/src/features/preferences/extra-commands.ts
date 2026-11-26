import type { Command } from '../../ui/commands';
import type { DocumentInfo } from '../../inspect';
import type { EditCommand } from '../../engine';
import { tr } from '../../i18n';

export function preferenceExtraCommands({ openUser, transfer, editable, info, edit }: {
  openUser: () => void; transfer: (mode: 'import' | 'export') => void; editable: boolean; info: DocumentInfo | null;
  edit: (label: string, commands: EditCommand[]) => unknown;
}): Record<string, Partial<Command>> {
  return {
    'prefs.key.quotes': {
      run: editable && info ? () => edit(tr('인용 부호 자동 사용'), [{ op: 'setDocPrefs', props: {
        'TextPreference.TypographersQuotes': !(info.documentPreferences?.['TextPreference.TypographersQuotes'] ?? true),
      } }]) : undefined,
      checked: Boolean(info?.documentPreferences?.['TextPreference.TypographersQuotes'] ?? true),
    },
    'id.file.user': { run: openUser },
    ...Object.fromEntries((['import', 'export'] as const).map(mode => ['id.file.userSettings.' + mode + 'UserSettings', { run: () => transfer(mode) }])),
    ...Object.fromEntries((['RGB', 'CMYK'] as const).map(space => ['id.edit.transparencyBlendSpace.document' + (space === 'RGB' ? 'Rgb' : 'Cmyk'), {
      run: editable ? () => edit(tr('투명도 혼합 공간'), [{ op: 'setDocPrefs', props: { 'TransparencyPreference.BlendingSpace': space } }]) : undefined,
      checked: info?.documentPreferences?.['TransparencyPreference.BlendingSpace'] === space,
    }])),
  };
}
