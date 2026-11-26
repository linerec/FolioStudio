import { tr } from '../../i18n';
import type { TrackedChange } from './types';
export type ChangeRow={story:string;index:number;change:TrackedChange};
type Props={rows:ChangeRow[];active:ChangeRow|undefined;pick:(r:ChangeRow)=>void;editable:boolean;tracking:boolean;toggle:(()=>void)|undefined;resolve:(accept:boolean)=>void};
export function ChangesPanel(p:Props) {
  const c=p.active?.change;
  return <section aria-label={tr('변경 내용 추적')} style={{padding:10,display:'grid',gap:8}}>
    <label><input type="checkbox" checked={p.tracking} disabled={!p.toggle} onChange={p.toggle}/>{tr('현재 스토리 추적')}</label>
    <select aria-label={tr('변경 목록')} size={7} value={p.active?`${p.active.story}:${p.active.index}`:''} onChange={e=>{const r=p.rows.find(r=>`${r.story}:${r.index}`===e.target.value);if(r)p.pick(r);}}>
      {p.rows.map(r=><option key={`${r.story}:${r.index}`} value={`${r.story}:${r.index}`}>{r.change.change_type==='InsertedText'?tr('삽입'):r.change.change_type==='DeletedText'?tr('삭제'):tr('이동')} — {r.change.text.slice(0,35)} — {r.change.user_name.replace(/^\$ID\//,'')}</option>)}
    </select>
    {c?<><div>{c.user_name.replace(/^\$ID\//,'')}</div><small>{c.date}</small><pre style={{whiteSpace:'pre-wrap'}}>{c.text}</pre>
      <div><button disabled={!p.editable} onClick={()=>p.resolve(true)}>{tr('변경 내용 수락')}</button> <button disabled={!p.editable} onClick={()=>p.resolve(false)}>{tr('변경 내용 거부')}</button></div>
    </>:<p>{tr('변경 기록이 없습니다.')}</p>}
  </section>;
}
