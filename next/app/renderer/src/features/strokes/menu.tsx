import type {Command} from '../../ui/commands';

/** Stroke panel menu > Stroke Styles…: the dialog is the engine's `strokeStyles` (a manager of the custom stroke styles), shown by the shared dialog frame. */
export function useStrokeStyles({editable,note}:{editable:boolean;note:(s:string)=>void}){
  const handlers:Record<string,Partial<Command>>={'id.panel.stroke.strokeStyles':{enabled:editable,run:async()=>{
    try{const {header}=await window.folio.request({cmd:'openDialog',dialog:'strokeStyles'});if(!header.ok)note(String(header.error));}catch(e){note(String(e));}
  }}};
  return {handlers};
}
