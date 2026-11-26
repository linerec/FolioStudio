import { useEffect, useState } from 'react';
import type { DocumentInfo } from '../../inspect';
import type { EditCommand, EditResult } from '../../engine';
import type { TextSelectionState } from '../../ui/textedit';
import type { Command } from '../../ui/commands';
import { tr } from '../../i18n';
import { timestamp } from './types';
import { NotesPanel } from './NotesPanel';

type Context = { info:DocumentInfo|null; editable:boolean; selection:TextSelectionState|null; user:string;
  edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;
  show:()=>void; reveal:(r:{story:string;start:number;end:number})=>void };
export function useNotes(ctx:Context) {
  const [picked,setPicked]=useState<{story:string;index:number}|null>(null);
  // the caret in the Notes panel's text box: Split Note works from it (InDesign: only a caret inside the note's text, rules/notes3-split.ko.md)
  const [caret,setCaret]=useState<{key:string;start:number;end:number;text:string}|null>(null);
  const rows=(ctx.info?.editorial??[]).flatMap(s=>s.notes.map((note,index)=>({story:s.id,index,note})));
  const active=rows.find(r=>r.story===picked?.story&&r.index===picked.index)??rows.find(r=>r.story===ctx.selection?.story&&r.note.at===ctx.selection.start);
  const story=ctx.selection?.story??active?.story;
  const pick=(row:typeof rows[number])=>{setPicked(row);ctx.show();ctx.reveal({story:row.story,start:row.note.at,end:row.note.at+1});};
  const run=(label:string,c:EditCommand)=>ctx.edit(label,[c]);
  const insertAt=async(story:string,at:number)=>{
    const index=rows.filter(r=>r.story===story&&r.note.at<at).length;
    if(await run(tr('새 메모'),{op:'insertNote',story,at,text:'',user:ctx.user,date:timestamp()})){setPicked({story,index});ctx.show();}
  };
  const selected=ctx.selection;
  const create=ctx.editable&&selected&&!selected.nested?()=>void insertAt(selected.story,selected.start):undefined;
  const convert=ctx.editable&&selected&&!selected.nested&&selected.start!==selected.end?async()=>{
    const index=rows.filter(r=>r.story===selected.story&&r.note.at<selected.start).length;
    if(await run(tr('메모로 변환'),{op:'convertToNote',story:selected.story,start:selected.start,end:selected.end,user:ctx.user,date:timestamp()})){setPicked({story:selected.story,index});ctx.show();}
  }:undefined;
  const action=(op:'deleteNote'|'noteToText',label:string)=>ctx.editable&&active?()=>void run(label,{op,story:active.story,index:active.index}):undefined;
  const next=(by:number)=>{if(rows.length){const i=active?rows.indexOf(active):(by>0?-1:0);pick(rows[(i+by+rows.length)%rows.length]);}};
  const here=active?`${active.story}:${active.index}`:'';
  const at=active&&caret&&caret.key===here&&caret.start===caret.end?caret.start:-1;
  // Split Note is on for a caret strictly inside the text (not at either end, no range): the note keeps the text before it
  const split=ctx.editable&&active&&caret&&at>0&&at<Array.from(caret.text).length?()=>{
    const dirty=caret.text!==active.note.text;
    void ctx.edit(tr('메모 분할'),[
      ...(dirty?[{op:'updateNote' as const,story:active.story,index:active.index,text:caret.text,date:timestamp()}]:[]),
      {op:'splitNote' as const,story:active.story,index:active.index,at,user:ctx.user,date:timestamp()}]);
    setCaret(null);
  }:undefined;
  const commands:Record<string,Partial<Command>>={
    'id.window.editorial.notes':{run:ctx.show},
    'id.type.notes.newNote':{run:create},
    'id.type.notes.notesMode':{run:convert??(active?()=>pick(active):create)},
    'id.type.notes.openNote':{run:active?()=>pick(active):undefined},
    'id.type.notes.deleteNote':{run:action('deleteNote',tr('메모 삭제'))},
    'id.type.notes.previousNote':{run:rows.length?()=>next(-1):undefined},
    'id.type.notes.nextNote':{run:rows.length?()=>next(1):undefined},
    'id.type.notes.convertToNote':{run:convert},
    'id.type.notes.convertToText':{run:action('noteToText',tr('텍스트로 변환'))},
    'id.type.notes.splitNote':{run:split},
    'id.type.notes.expandCollapseNotesInStory':{run:ctx.editable&&story?()=>void run(tr('메모 펼침 상태'),{op:'collapseStoryNotes',story,collapsed:rows.some(r=>r.story===story&&!r.note.collapsed)}):undefined},
    'id.type.notes.removeNotesFromStory':{run:ctx.editable&&story?()=>void run(tr('스토리에서 메모 제거'),{op:'removeNotes',story,document:false}):undefined},
    'id.type.notes.removeAllNotes':{run:ctx.editable&&rows.length?()=>void run(tr('모든 메모 제거'),{op:'removeNotes',story:rows[0].story,document:true}):undefined},
  };
  useEffect(()=>{if(!ctx.info)setPicked(null);},[ctx.info]);
  return {commands,insertAt,panel:<NotesPanel rows={rows} active={active} pick={pick} editable={ctx.editable} create={create}
    save={(text)=>active&&void run(tr('메모 편집'),{op:'updateNote',story:active.story,index:active.index,text,date:timestamp()})}
    collapse={()=>active&&void run(tr('메모 펼침 상태'),{op:'collapseNote',story:active.story,index:active.index,collapsed:!active.note.collapsed})}
    remove={action('deleteNote',tr('메모 삭제'))} onCaret={c=>setCaret(c&&active?{key:here,...c}:null)} />};
}
