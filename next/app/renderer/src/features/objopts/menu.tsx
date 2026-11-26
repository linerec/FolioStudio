import { tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { ItemStore } from '../../inspect';
import { findItem, type SelEntry } from '../../ui/overlay';
import type { Command } from '../../ui/commands';

type Context={page?:string;editable:boolean;selection:SelEntry[];store:ItemStore;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>};
export function useObjectOptions(ctx:Context) {
  const targets=ctx.selection.map(s=>findItem(ctx.store.peek(s.page),s.id)?.item).filter(i=>i&&i.kind!=='group');
  const enabled=ctx.editable&&targets.length===ctx.selection.length&&targets.length>0;
  const handlers:Record<string,Partial<Command>>={
    // the engine session holds the dialog (next/shared/dialogs/objectLayerOptions.json); the engine turns the comp and the layers into commands
    'object.layerOptions':{enabled:enabled&&targets.length===1&&['PDF','ImportedPage','Image'].includes(targets[0]?.content?.kind??'')&&!!targets[0]?.content?.layer_options,run:()=>void window.folio.request({cmd:'openDialog',dialog:'objectLayerOptions',ids:[targets[0]!.id]}).catch(()=>{})},
    // the engine session holds the dialog (next/shared/dialogs/imageColor.json); it lists this computer's profiles of the picture's space
    'id.object.imageColorSettings':{enabled:enabled&&targets.length===1&&!!targets[0]?.content?.image_color,run:()=>void window.folio.request({cmd:'openDialog',dialog:'imageColor',ids:[targets[0]!.id]}).catch(()=>{})},
    // the engine session holds the dialog (next/shared/dialogs/qrCode.json); the new group is selected as the edit's created item
    'id.object.generateQrCode':{enabled:ctx.editable&&!!ctx.page,run:()=>{if(ctx.page)void window.folio.request({cmd:'openDialog',dialog:'qrCode',ids:[ctx.page]}).catch(()=>{});}},
    // the engine session holds the dialog (next/shared/dialogs/frameFitting.json); it starts from the first frame's options
    'object.fittingOptions':{enabled,run:()=>void window.folio.request({cmd:'openDialog',dialog:'frameFitting',ids:targets.map(i=>i!.id)}).catch(()=>{})},
    'id.object.fitting.clearFrameFittingOptions':{enabled,run:()=>void ctx.edit(tr('프레임 맞춤 옵션 지우기'),[{op:'clearFrameFitting',ids:targets.map(i=>i!.id)}])},
    'id.object.clippingPath.convertClippingPathToFrame':{enabled:enabled&&targets.every(i=>i?.content?.clipping),run:()=>void ctx.edit(tr('클리핑 패스를 프레임으로 변환'),[{op:'convertClippingToFrame',ids:targets.map(i=>i!.id)}])},
    // the engine session holds the dialog (next/shared/dialogs/clippingOptions.json); it starts from the graphic's options
    'id.object.clippingPath.options':{enabled:enabled&&targets.length===1&&!!targets[0]?.content?.clipping_options,run:()=>void window.folio.request({cmd:'openDialog',dialog:'clippingOptions',ids:[targets[0]!.id]}).catch(()=>{})},
  };
  return {handlers};
}
