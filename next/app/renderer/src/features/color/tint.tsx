import { useRef, useState } from 'react';
import { tr } from '../../i18n';
import type { EditCommand } from '../../engine';
import { NumberField } from '../../ui/controls/numberfield';
import { usePreviewEdit } from '../../ui/controls/usePreviewEdit';

/**
 * The Color panel's tint: a number box and a slider for the same value. `build` says which commands give the colour that tint;
 * a value tried in either (an arrow, a wheel turn, a dragged slider) is shown by the engine's preview session and made one undo step
 * when it is chosen, taken back exactly when it is not (ui/controls/README.ko.md).
 */
export function TintFields({value, disabled, build, edit}: {value:number; disabled:boolean; build:(value:number)=>EditCommand[]; edit:(label:string,commands:EditCommand[])=>unknown}) {
  const handlers=usePreviewEdit(tr('색조'),build,edit);
  // the slider's value while it is held: it follows the pointer at once, the document a moment later
  const [slide,setSlide]=useState<number|null>(null);
  const from=useRef<number|null>(null);
  const settle=()=>{
    const v=slide,base=from.current;
    from.current=null;setSlide(null);
    if(v===null)return;
    if(base!==null&&Math.abs(v-base)<1e-9)handlers.onCancel();else handlers.onCommit(v);
  };
  return <fieldset className="color-tint" disabled={disabled}>
    <label>{tr('색조')}<NumberField value={value} label={tr('색조')} min={0} max={100} step={1} decimals={2} width={64} disabled={disabled} isolateKeys {...handlers}/>%</label>
    <input aria-label={tr('색조 슬라이더')} type="range" min={0} max={100} step={0.1} value={slide??value}
      onChange={e=>{const v=Number(e.target.value);if(from.current===null)from.current=value;setSlide(v);handlers.onPreview(v,from.current);}}
      onPointerUp={settle} onKeyUp={settle} onBlur={settle}/>
  </fieldset>;
}
