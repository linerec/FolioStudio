import { useCallback, useMemo, useRef, useState } from 'react';
import type { Command } from './commands';
import type { RowAction } from './stylecontext';

export type ObjectStyleAction={id:string;disabled:boolean;run:()=>unknown;/** a menu check */checked?:boolean;/** only in the panel menu, not a button of the panel */menuOnly?:boolean};
const registry:Record<string,string>={new:'newObjectStyle',options:'styleOptions',duplicate:'duplicateObjectStyle',delete:'deleteStyles',
  redefine:'redefineStyle',clear:'clearOverrides',clearUndefined:'clearAttributesNotDefinedByStyle',
  clearOnApply:'clearOverridesWhenApplyingStyle',newGroup:'newStyleGroup',copyToGroup:'copyToGroup',newGroupFromStyles:'newGroupFromStyles',sort:'sortByName',selectUnused:'selectAllUnused',breakLink:'breakLinkToStyle',openAll:'openAllGroups',closeAll:'closeAllGroups',small:'smallPanelRows',load:'loadObjectStyles'};
// Default Text Frame Style / Default Graphic Frame Style: [None], [Basic Graphics Frame], [Basic Text Frame]
for (const kind of ['text','graphic']) for (const choice of ['none','basicGraphicsFrame','basicTextFrame'])
  registry[`default.${kind}.${choice}`]=`${kind==='text'?'defaultTextFrameStyle':'defaultGraphicFrameStyle'}.${choice}`;

/** The commands only the row's right-click menu has (menutree.ts CONTEXT_MENUS.styleObject): Apply Style and its three ways of clearing. */
const context:Record<string,string>={apply:'id.ctx.styleObject.applyStyle',applyClear:'id.ctx.styleObject.applyStyleClearOverrides',
  applyUndefined:'id.ctx.styleObject.applyStyleClearAttributesNotDefinedByStyle',applyAll:'id.ctx.styleObject.applyStyleClearAll',
  groupOptions:'id.ctx.styleObject.styleGroupOptions',duplicateGroup:'id.ctx.styleObject.duplicateGroup',deleteGroup:'id.ctx.styleObject.deleteGroup'};

/** What the right-click menu's commands do: the open panel's actions by the command ids of the menu (stylecontext.tsx). */
export function objectStyleRowActions(actions:ObjectStyleAction[]):Record<string,RowAction>{
  const out:Record<string,RowAction>={};
  for(const a of actions){
    const id=context[a.id]??(registry[a.id]?'id.panel.objectStyles.'+registry[a.id]:'');
    if(id)out[id]={disabled:a.disabled,checked:a.checked,run:a.run};
  }
  return out;
}

/** The panel owns selection and dialogs; this registry supplies its existing panel menu. */
export function useObjectStyleMenu() {
  const current=useRef<ObjectStyleAction[]>([]),signature=useRef('');
  const [revision,setRevision]=useState(0);
  const publish=useCallback((actions:ObjectStyleAction[])=>{
    current.current=actions;
    const next=JSON.stringify(actions.map(a=>[a.id,a.disabled,a.checked]));
    if(next!==signature.current){signature.current=next;setRevision(r=>r+1);}
  },[]);
  const handlers=useMemo(()=>Object.fromEntries([...Object.entries(registry).map(([key,command])=>[key,'id.panel.objectStyles.'+command] as const),...Object.entries(context)].map(([key,id])=>[
    id,
    {enabled:current.current.some(a=>a.id===key&&!a.disabled),checked:current.current.find(a=>a.id===key)?.checked,run:()=>{const action=current.current.find(a=>a.id===key);if(action&&!action.disabled)action.run();}} satisfies Partial<Command>
  ])),[revision]);
  return {publish,handlers};
}
