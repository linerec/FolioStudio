import { useEffect, useState } from 'react';
import type { DocumentInfo } from '../../inspect';
import type { EditCommand, EditResult } from '../../engine';
import type { TextSelectionState } from '../../ui/textedit';
import type { Command } from '../../ui/commands';
import { tr } from '../../i18n';
import { ChangesPanel, type ChangeRow } from './ChangesPanel';
type Context={info:DocumentInfo|null;editable:boolean;selection:TextSelectionState|null;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;show:()=>void;reveal:(r:{story:string;start:number;end:number})=>void};
export function useChanges(ctx:Context) {
  const [picked,setPicked]=useState<{story:string;index:number}|null>(null);
  const [pending,setPending]=useState<{story:string;index:number}|null>(null);
  const stories=ctx.info?.editorial??[];
  const rows=stories.flatMap(s=>s.changes.map((change,index)=>({story:s.id,index,change})));
  const active=rows.find(r=>r.story===picked?.story&&r.index===picked.index)??rows.find(r=>r.story===ctx.selection?.story&&r.change.start<=ctx.selection.start&&r.change.end>=ctx.selection.end);
  const story=ctx.selection?.story??active?.story;
  const tracking=stories.find(s=>s.id===story)?.trackChanges??false;
  const pick=(r:ChangeRow)=>{setPicked(r);ctx.show();ctx.reveal({story:r.story,start:r.change.start,end:r.change.end});};
  const next=(by:number)=>{if(rows.length){const i=active?rows.indexOf(active):(by>0?-1:0);pick(rows[(i+by+rows.length)%rows.length]);}};
  const setTracking=(document:boolean,enabled:boolean)=>ctx.editable&&(document?stories.length:story)?()=>void ctx.edit(tr('변경 내용 추적'),[{op:'setTrackChanges',story:story??stories[0].id,document,enabled}]):undefined;
  const resolve=async(accept:boolean,document=false,all=false,user='',findNext=false)=>{
    const owner=document?(story??rows[0]?.story):all?story:active?.story;
    if(!ctx.editable||!owner||(!all&&!active))return;
    const following=findNext&&active?rows[rows.indexOf(active)+1]:undefined;
    const result=await ctx.edit(accept?tr('변경 내용 수락'):tr('변경 내용 거부'),[{op:'resolveChanges',story:owner,document,index:all?-1:active!.index,user,accept}]);
    if(result&&following){setPending({story:following.story,index:following.index-(following.story===active?.story?1:0)});ctx.show();}
  };
  const commands:Record<string,Partial<Command>>={
    'id.window.editorial.trackChanges':{run:ctx.show},
    'id.type.trackChanges.trackChangesInCurrentStory':{run:setTracking(false,!tracking),checked:tracking},
    'id.type.trackChanges.enableTrackingInAllStories':{run:setTracking(true,true)},
    'id.type.trackChanges.disableTrackingInAllStories':{run:setTracking(true,false)},
    'id.type.trackChanges.previousChange':{run:rows.length?()=>next(-1):undefined},
    'id.type.trackChanges.nextChange':{run:rows.length?()=>next(1):undefined},
  };
  for(const accept of [true,false]) {
    const verb=accept?'accept':'reject';
    commands[`id.type.trackChanges.${verb}Change`]={run:ctx.editable&&active?()=>void resolve(accept):undefined};
    commands[`id.type.trackChanges.${verb}ChangeFindNext`]={run:ctx.editable&&active?()=>void resolve(accept,false,false,'',true):undefined};
    for(const document of [false,true])for(const byUser of [false,true]) {
      commands[`id.type.trackChanges.${verb}AllChanges${byUser?'ByThisUser':''}.inThis${document?'Document':'Story'}`]={run:ctx.editable&&rows.length&&(document||story)&&(!byUser||active)?()=>void resolve(accept,document,true,byUser?active!.change.user_name:''):undefined};
    }
  }
  useEffect(()=>{if(!ctx.info)setPicked(null);},[ctx.info]);
  useEffect(()=>{if(pending){const r=rows.find(r=>r.story===pending.story&&r.index===pending.index);if(r)pick(r);setPending(null);}},[ctx.info,pending]);
  return {commands,panel:<ChangesPanel rows={rows} active={active} pick={pick} editable={ctx.editable} tracking={tracking} toggle={setTracking(false,!tracking)} resolve={accept=>void resolve(accept)}/>};
}
