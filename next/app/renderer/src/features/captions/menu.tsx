import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import type { EditCommand, EditResult, LinkRow } from '../../engine';
import { pageOf, type DocumentInfo, type ItemStore } from '../../inspect';
import { findItem, type SelEntry } from '../../ui/overlay';
import type { TextSelectionState } from '../../ui/textedit';
import type { Command } from '../../ui/commands';

type Context={info:DocumentInfo|null;editable:boolean;selection:SelEntry[];text:TextSelectionState|null;link:LinkRow|null;store:ItemStore;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;note:(s:string)=>void};
export function useCaptions(ctx:Context) {
  const [range,setRange]=useState('');
  const key=ctx.text?`${ctx.text.story}:${ctx.text.start}:${ctx.text.end}`:'';
  useEffect(()=>{let alive=true;setRange('');const t=ctx.text;if(t&&t.start<t.end&&t.variables)void window.folio.request({cmd:'captionRange',story:t.story,start:t.start,end:t.end}).then(({header})=>{if(alive&&header.ok&&header.live)setRange(key);}).catch(()=>{});return()=>{alive=false;};},[key,ctx.info]);
  const targets=ctx.selection.filter(s=>!!findItem(ctx.store.peek(s.page),s.id)?.item.content?.link);
  const generate=async(live:boolean,links:boolean)=>{
    const selected=links&&ctx.link&&ctx.link.page!==null?[{page:ctx.link.page,id:ctx.link.owner}]:targets;
    const pages=new Map<number,string[]>();for(const s of selected)pages.set(s.page,[...(pages.get(s.page)??[]),s.id]);
    const commands:EditCommand[]=[];for(const [p,ids] of pages){const page=pageOf(ctx.info,p);if(page)commands.push({op:'generateCaptions',ids,page:page.id,live});}
    if(commands.length)await ctx.edit(live?tr('라이브 캡션 생성'):tr('정적 캡션 생성'),commands);
  };
  const handlers:Record<string,Partial<Command>>={
    // the engine session holds the dialog (next/shared/dialogs/captionSetup.json); it starts from the document's setup
    'id.object.captions.captionSetup':{enabled:ctx.editable&&!!ctx.info,run:async()=>{const {header}=await window.folio.request({cmd:'openDialog',dialog:'captionSetup'});if(!header.ok)ctx.note(String(header.error));}},
    'id.object.captions.convertToStaticCaption':{enabled:ctx.editable&&!!key&&range===key,run:()=>{const t=ctx.text;if(t)void ctx.edit(tr('정적 캡션으로 변환'),[{op:'convertToStaticCaption',story:t.story,start:t.start,end:t.end}]);}},
  };
  for(const [id,live,links] of [
    ['id.object.captions.generateLiveCaption',true,false],['id.object.captions.generateStaticCaption',false,false],
    ['id.panel.links.captions.generateLiveCaptions',true,true],['id.panel.links.captions.generateStaticCaptions',false,true],
  ] as const)handlers[id]={enabled:ctx.editable&&(links?!!ctx.link&&ctx.link.page!==null:targets.length>0),run:()=>void generate(live,links)};
  return {handlers};
}
