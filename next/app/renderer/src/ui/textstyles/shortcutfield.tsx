import { t as tr } from '../../i18n';
import { useRef, useState } from 'react';
import { getStyleShortcut } from '../../engine';
import { shortcutSpecFromEvent } from '../styleshortcuts';
import { commandOwner } from '../commandowner';

/** Style Options "Shortcut" field (measured in InDesign 2025, oracle/rules/stylekeys.ko.md): a key press replaces the content with the
 *  combination; one the engine does not accept (a bare key) blanks the field; Backspace/Delete clears it; under it "Currently assigned to"
 *  names ONE holder (a duplicate is only a warning): the style that wins a key press among those holding the combination (no line when that is the style
 *  being edited), else the menu command that uses it (a style that holds it hides the command), else "[unassigned]" (measured: rules/stylekeys-c2.ko.md).
 *  Tab, Enter and Escape keep their dialog meaning. */
export function StyleShortcutField({kind,selfId,spec,label,disabled,onChange}:{kind:'paragraph'|'character';selfId?:string;spec:string;label:string;disabled:boolean;onChange:(spec:string,label:string)=>void}) {
  const [owners,setOwners]=useState<string|null>(null);
  const turn=useRef(0);
  const keyDown=(e:React.KeyboardEvent<HTMLInputElement>)=>{
    if(disabled||e.key==='Tab'||e.key==='Enter'||e.key==='Escape')return;
    e.preventDefault();
    if(['Control','Shift','Alt','Meta','AltGraph','CapsLock','NumLock'].includes(e.key))return;
    const mine=++turn.current;
    if(e.key==='Backspace'||e.key==='Delete'){setOwners(null);onChange('','');return;}
    const next=shortcutSpecFromEvent(e.nativeEvent);
    if(!next){setOwners(null);onChange('','');return;}
    getStyleShortcut(next).then(({info})=>{
      if(mine!==turn.current)return;
      // Ctrl+A, C, V, X, Z are the field's own editing keys: the field stays as it was (measured); every other refused key blanks it
      if(info.error?.startsWith('reserved'))return;
      if(!info.valid||!info.shortcut){setOwners(null);onChange('','');return;}
      // the style that wins a key press is named; when that is this very style InDesign shows no line at all (measured, m5b)
      const winner=info.owners[0];
      setOwners(winner?(winner.kind===kind&&winner.id===selfId?null:winner.name):commandOwner(info.shortcut)?.name??'');
      onChange(info.shortcut,info.label??'');
    }).catch(()=>{if(mine===turn.current){setOwners(null);onChange('','');}});
  };
  return <>
    <label>{tr('단축키')}<input aria-label={tr('스타일 단축키')} data-shortcut={spec} readOnly disabled={disabled} value={label} onKeyDown={keyDown} onPaste={e=>e.preventDefault()}/></label>
    {owners!==null&&<p className="style-shortcut-owner"><b>{tr('현재 할당:')}</b> {owners||tr('[할당 안 됨]')}</p>}
  </>;
}
