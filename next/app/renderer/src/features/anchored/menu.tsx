import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { ItemStore } from '../../inspect';
import { findItem, type SelEntry } from '../../ui/overlay';
import type { Command } from '../../ui/commands';

export type AnchoredObject={id:string;story:string;at:number;props:Record<string,string|number|boolean>;fields:{key:string;choices:string[]}[]};
type Context={editable:boolean;selection:SelEntry[];store:ItemStore;text:{story:string;start:number;end:number;nested?:unknown}|null;revision:unknown;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>};
export function useAnchoredObjects(ctx:Context) {
  const [object,setObject]=useState<AnchoredObject|null>(null);
  const item=ctx.selection.length===1?findItem(ctx.store.peek(ctx.selection[0].page),ctx.selection[0].id)?.item:undefined;
  const id=item?.anchored?item.id:undefined;
  const story=ctx.text&&!ctx.text.nested?ctx.text.story:undefined,at=ctx.text?Math.min(ctx.text.start,ctx.text.end):0;
  useEffect(()=>{let live=true;setObject(null);if(ctx.editable&&(id||story))void window.folio.request({cmd:'anchoredObject',...(id?{object:id}:{story,at})}).then(({header})=>{if(live&&header.ok)setObject((header.object as AnchoredObject)??null);}).catch(()=>{});return()=>{live=false;};},[ctx.editable,id,story,at,ctx.revision]);
  const handlers:Record<string,Partial<Command>>={
    'id.object.anchoredObject.options':{enabled:!!object,run:()=>object&&open(object.id)},
    'id.object.anchoredObject.insert':{enabled:!!story&&ctx.editable&&ctx.text?.start===ctx.text?.end,run:()=>{if(story)void window.folio.request({cmd:'openDialog',dialog:'insertAnchored',values:{story,at}}).catch(()=>{});}},
    'id.object.anchoredObject.release':{enabled:object?.props.anchoredPosition==='Anchored',run:()=>{if(object)void ctx.edit(tr('고정 개체 풀기'),[{op:'releaseAnchoredObject',ids:[object.id]}]);}},
  };
  /** The options dialog on an object (the Alt drag of the anchored object control opens it on the object just anchored). */
  const open=(objectId:string)=>{void window.folio.request({cmd:'openDialog',dialog:'anchoredOptions',ids:[objectId]}).catch(()=>{});};
  return {handlers,open};
}
