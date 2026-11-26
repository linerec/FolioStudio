import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { declarePanelCommands, usePanelCommands } from '../../ui/panelbus';

const GENERATE = 'id.panel.index.generateIndex';
declarePanelCommands([GENERATE]);

/** The Generate Index button and menu item; the dialog (title, font, styles) is the engine's `generateIndex`, shown by the shared dialog frame. */
export function GenerateIndex({ctx,disabled}:{ctx:PanelCtx;disabled:boolean}) {
  const open=()=>{void window.folio.request({cmd:'openDialog',dialog:'generateIndex',ids:[ctx.doc?.pages[ctx.current]?.id??''],values:{title:tr('색인')}}).catch(()=>{});};
  usePanelCommands('index.generate',{[GENERATE]:disabled?undefined:open});
  return <button type="button" disabled={disabled} onClick={open}>{tr('색인 만들기')}</button>;
}
