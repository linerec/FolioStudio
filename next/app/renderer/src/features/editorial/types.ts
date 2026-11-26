export type EditorialNote = { at:number; text:string; collapsed:boolean; user_name:string; creation_date:string; modification_date:string; name:string|null };
export type TrackedChange = { start:number; end:number; text:string; change_type:string; user_name:string; date:string };
export type EditorialStory = { id:string; notes:EditorialNote[]; changes:TrackedChange[]; trackChanges:boolean };
export function timestamp() {
  const d=new Date(), p=(n:number)=>String(n).padStart(2,'0');
  return `${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
