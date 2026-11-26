import type { Command } from '../../ui/commands';
import type { DocumentInfo } from '../../inspect';
import type { EditCommand } from '../../engine';
import { tr } from '../../i18n';
import { SCREEN_MODES } from './screen';
import type { ScreenMode } from '../../ui/tools';

export function viewCommands(p: {
  mode: ScreenMode;
  setMode: (mode: ScreenMode) => void;
  contentGrabber: boolean;
  toggleGrabber: () => void;
  editable: boolean;
  info: DocumentInfo | null;
  edit: (label: string, commands: EditCommand[]) => unknown;
}): Record<string, Partial<Command>> {
  const match = p.info?.view?.pasteboard.matchPreviewBackground ?? true;
  return {
    ...Object.fromEntries(SCREEN_MODES.map(mode => [`view.mode.${mode}`, { run: () => p.setMode(mode), checked: p.mode === mode }])),
    'view.contentGrabber': { run: p.toggleGrabber, checked: p.contentGrabber },
    'id.view.matchPasteboardToThemeColor': {
      run: p.editable && p.info?.view ? () => { p.edit(tr('작업 영역 설정'), [{ op: 'setViewPref', key: 'matchPreviewBackground', value: !match }]); } : undefined,
      checked: match,
    },
  };
}
