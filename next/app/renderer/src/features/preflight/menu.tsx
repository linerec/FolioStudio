import {tr} from '../../i18n';
import type {Command} from '../../ui/commands';

/** Preflight panel menu: Define Profiles and Preflight Options are the engine's dialogs (next/shared/dialogs/preflightProfiles.json,
 *  preflightOptions.json), shown by the shared dialog frame. */
export function usePreflightProfiles({editable,note}:{editable:boolean;note:(s:string)=>void}){
  const open=(dialog:string,values?:Record<string,unknown>)=>async()=>{
    try {const {header}=await window.folio.request({cmd:'openDialog',dialog,...(values?{values}:{})});if(!header.ok)note(String(header.error));}catch(e){note(String(e));}
  };
  const handlers:Record<string,Partial<Command>>={
    // the words a new profile is named with are the screen's
    'id.panel.preflight.defineProfiles':{enabled:editable,run:open('preflightProfiles',{namePrefix:tr('새 사전 검사 프로필')})},
    'id.panel.preflight.preflightOptions':{enabled:editable,run:open('preflightOptions')},
  };
  return {handlers};
}
