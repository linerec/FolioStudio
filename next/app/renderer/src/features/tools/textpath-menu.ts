import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { DocumentInfo } from '../../inspect';
import type { SelEntry } from '../../ui/overlay';
import type { Command } from '../../ui/commands';

export function useTextPathMenu(ctx:{info:DocumentInfo|null;editable:boolean;selection:SelEntry[];story?:string;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>}) {
  const [items,setItems]=useState<string[]>([]);
  useEffect(()=>{
    let live=true;setItems([]);
    if(ctx.info&&ctx.editable)void window.folio.request({cmd:'textPathItems',story:ctx.story}).then(async({header})=>{
      if(!header.ok)return;
      let found=header.ids as string[];
      if(ctx.story){
        const supported=await Promise.all(found.map(async id=>(await window.folio.request({cmd:'textPathOptions',target:id})).header.supported));
        found=found.filter((_,i)=>supported[i]);
      }
      if(live)setItems(found);
    }).catch(()=>{});
    return()=>{live=false;};
  },[ctx.info,ctx.editable,ctx.story]);
  const ids=ctx.story?(items.length===1?items:[]):ctx.selection.map(s=>s.id);
  const handlers:Record<string,Partial<Command>>={
    'id.type.typeOnAPath.deleteTypeFromPath':{enabled:ctx.editable&&ids.length>0&&ids.every(id=>items.includes(id)),run:()=>void ctx.edit(tr('패스에서 문자 삭제'),[{op:'removeTextFromPath',ids}])},
    'id.type.typeOnAPath.options':{enabled:ctx.editable&&ids.length===1&&items.includes(ids[0]),run:()=>void window.folio.request({cmd:'openDialog',dialog:'textPathOptions',ids:[ids[0]]}).catch(()=>{})},
  };
  return {handlers};
}
