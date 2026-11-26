import { useState } from 'react';
import { tr } from '../../i18n';
import type { EditorialNote } from './types';
type Row={story:string;index:number;note:EditorialNote};
type Props={rows:Row[];active:Row|undefined;pick:(r:Row)=>void;editable:boolean;create:(()=>void)|undefined;save:(text:string)=>void;collapse:()=>void;remove:(()=>void)|undefined;
  /** Where the caret is in the text box (offsets in the note's characters, a return counts one) and the text shown, or null when the box is left empty. Split Note works from it. */
  onCaret?:(c:{start:number;end:number;text:string}|null)=>void};
const chars=(s:string)=>Array.from(s).length;
const draftOf=(active:Row|undefined)=>active?.note.text.replace(/\r/g,'\n')??'';
export function NotesPanel(p:Props) {
  // The draft starts from the active note's text and starts over when another note (or its saved text) becomes active.
  // That reset happens while rendering, not in an effect: an effect runs only after the textarea has been committed, and
  // a keystroke that arrives in between (the UI check's input event) was then overwritten by the late reset.
  const [draft,setDraft]=useState(()=>draftOf(p.active));
  const key=`${p.active?.story}\u0000${p.active?.index}\u0000${p.active?.note.text}`;
  const [draftKey,setDraftKey]=useState(key);
  if(draftKey!==key){setDraftKey(key);setDraft(draftOf(p.active));}
  const n=p.active?.note;
  const report=(t:HTMLTextAreaElement)=>p.onCaret?.({start:chars(t.value.slice(0,t.selectionStart)),end:chars(t.value.slice(0,t.selectionEnd)),text:t.value.replace(/\n/g,'\r')});
  return <section aria-label={tr('메모')} style={{padding:10,display:'grid',gap:8}}>
    <button disabled={!p.create} onClick={p.create}>{tr('새 메모')}</button>
    <select aria-label={tr('메모 목록')} size={5} value={p.active?`${p.active.story}:${p.active.index}`:''} onChange={e=>{const r=p.rows.find(r=>`${r.story}:${r.index}`===e.target.value);if(r)p.pick(r);}}>
      {p.rows.map(r=><option key={`${r.story}:${r.index}`} value={`${r.story}:${r.index}`}>{r.note.name||r.note.text.slice(0,45)||tr('빈 메모')} — {r.note.user_name}</option>)}
    </select>
    {n?<><div>{n.user_name}</div><small>{tr('만든 날짜')}: {n.creation_date}<br/>{tr('수정 날짜')}: {n.modification_date}</small>
      <button disabled={!p.editable} onClick={p.collapse}>{n.collapsed?tr('메모 펼치기'):tr('메모 접기')}</button>
      {!n.collapsed&&<><textarea aria-label={tr('메모 내용')} rows={6} disabled={!p.editable} value={draft}
        onChange={e=>{setDraft(e.target.value);report(e.currentTarget);}} onSelect={e=>report(e.currentTarget)} />
      <button disabled={!p.editable||draft===n.text.replace(/\r/g,'\n')} onClick={()=>p.save(draft.replace(/\n/g,'\r'))}>{tr('적용')}</button></>}
      <button disabled={!p.remove} onClick={p.remove}>{tr('메모 삭제')}</button></>:<p>{p.rows.length?tr('목록에서 메모를 고르세요.'):tr('메모가 없습니다.')}</p>}
  </section>;
}
