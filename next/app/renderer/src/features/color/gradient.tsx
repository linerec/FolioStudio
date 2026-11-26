import { useEffect, useRef, useState } from 'react';
import { tr } from '../../i18n';
import type { Swatch } from '../../ui/swatchedit';
import type { PanelCtx } from '../../ui/panels';
import { useSelected } from '../../ui/panels';
import { PreviewNumberField } from '../../ui/controls/previewnumberfield';
import { NumberField } from '../../ui/controls/numberfield';

export type Stop = {color: string; location: number; midpoint: number};
export const defaultStops: Stop[] = [{color:'Color/Paper',location:0,midpoint:50},{color:'Color/Black',location:100,midpoint:50}];
export function ramp(stops: Stop[], swatches: Swatch[]) {
  return `linear-gradient(90deg, ${stops.map(s => `rgb(${swatches.find(c => c.id === s.color)?.rgb.join(',') ?? '0,0,0'}) ${s.location}%`).join(',')})`;
}

export function StopEditor({stops, change, swatches, disabled=false, pick, shared=false}: {stops: Stop[]; change: (stops: Stop[]) => void; swatches: Swatch[]; disabled?: boolean; /** The picked stop kept by the caller (a panel that is redrawn between edits). */ pick?: [number, (n: number) => void];
  /** The panel's position and midpoint boxes take Enter themselves; the dialog's (not `shared`) leave Enter to the form's OK (`inDialog`). */ shared?: boolean}) {
  const local=useState(0);
  const [picked,setPicked]=pick??local;
  const index=Math.min(picked,stops.length-1), stop=stops[index];
  const update=(props: Partial<Stop>)=>{
    const next=stops.map((s,i)=>i===index?{...s,...props}:s);
    const selected=next[index];next.sort((a,b)=>a.location-b.location);setPicked(next.indexOf(selected));change(next);
  };
  return <fieldset disabled={disabled} className="gradient-stops" aria-label={tr('그레이디언트 정지점')}>
    <div style={{height:20,background:ramp(stops,swatches),border:'1px solid var(--border)'}} />
    <div style={{position:'relative',height:28,margin:'0 8px'}}>{stops.map((s,i)=><button key={i} type="button" aria-label={tr('정지점 {0}',{0:i+1})} aria-pressed={index===i}
      style={{position:'absolute',left:`${s.location}%`,transform:'translateX(-50%)',padding:'0 3px',outline:index===i?'1px solid var(--accent)':''}}
      onClick={()=>setPicked(i)}>◆</button>)}</div>
    <label>{tr('정지점 색상')}<select aria-label={tr('정지점 색상')} value={stop.color} onChange={e=>update({color:e.target.value})}>
      {swatches.filter(s=>['process','spot','tint','paper'].includes(s.model)).map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
    </select></label>
    <label>{tr('위치')}<NumberField value={stop.location} label={tr('정지점 위치')} min={0} max={100} step={1} decimals={2} width={64} disabled={disabled} isolateKeys inDialog={!shared} onCommit={v=>update({location:v})}/>%</label>
    <input aria-label={tr('정지점 슬라이더')} type="range" min={0} max={100} step={0.1} value={stop.location} onChange={e=>update({location:e.target.valueAsNumber})}/>
    <label>{tr('중간점')}<NumberField value={stop.midpoint} label={tr('중간점')} min={13} max={87} step={1} decimals={2} width={64} disabled={index===0||disabled} isolateKeys inDialog={!shared} onCommit={v=>update({midpoint:v})}/>%</label>
    <div><button type="button" onClick={()=>{const location=(stop.location+(stops[index+1]?.location??100))/2;const next=[...stops,{...stop,location,midpoint:50}].sort((a,b)=>a.location-b.location);setPicked(next.findIndex(s=>s.location===location));change(next);}}>{tr('정지점 추가')}</button>
      <button type="button" disabled={disabled||stops.length<=2} onClick={()=>{change(stops.filter((_,i)=>i!==index));setPicked(Math.max(0,index-1));}}>{tr('정지점 삭제')}</button>
      <button type="button" onClick={()=>{change(stops.map((s,i)=>({...s,location:100-s.location,midpoint:i===stops.length-1?50:100-stops[i+1].midpoint})).reverse());setPicked(stops.length-1-index);}}>{tr('반전')}</button></div>
  </fieldset>;
}

/** The gradient swatch of the panel's paint, edited in place: its type, and its stops on the ramp (one edit once the typing or the dragging settles). */
function GradientEditor({ctx, gradient, pick}: {ctx:PanelCtx; gradient:Swatch; pick:[number,(n:number)=>void]}) {
  const stops=gradient.stops??defaultStops, signature=JSON.stringify(stops);
  const [draft,setDraft]=useState<Stop[]>(stops);
  const pending=useRef<{id:string;stops:Stop[]}|null>(null), timer=useRef(0);
  const flush=()=>{window.clearTimeout(timer.current);const p=pending.current;pending.current=null;if(p)void ctx.edit(tr('그레이디언트 정지점'),[{op:'editGradient',id:p.id,stops:p.stops}]);};
  useEffect(()=>{setDraft(stops);},[signature,gradient.id]);
  useEffect(()=>flush,[]);
  const change=(next:Stop[])=>{setDraft(next);pending.current={id:gradient.id,stops:next};window.clearTimeout(timer.current);timer.current=window.setTimeout(flush,450);};
  return <>
    <label>{tr('유형')}<select aria-label={tr('그레이디언트 유형')} disabled={!ctx.editable} value={gradient.space} onChange={e=>void ctx.edit(tr('그레이디언트 유형'),[{op:'editGradient',id:gradient.id,kind:e.target.value}])}>
      <option value="Linear">{tr('선형')}</option><option value="Radial">{tr('방사형')}</option></select></label>
    <StopEditor stops={draft} change={change} swatches={ctx.info!.swatches} disabled={!ctx.editable} pick={pick} shared/>
  </>;
}

export function GradientPanel({ctx}: {ctx:PanelCtx}) {
  const selected=useSelected(ctx);
  const [stroke,setStroke]=useState(false);
  const pickedStop=useState(0);
  const paint=stroke?selected?.item.stroke:selected?.item.fill;
  const gradient=ctx.info?.swatches.find(s=>s.model==='gradient'&&s.name===paint?.swatch);
  const geo=stroke?selected?.item.stroke_gradient:selected?.item.fill_gradient;
  const enabled=ctx.editable&&ctx.selection.length===1&&!!gradient&&!!selected&&!selected.entry.content;
  return <div className="p-form gradient-panel">
    <label>{tr('적용 대상')}<select aria-label={tr('적용 대상')} value={stroke?'stroke':'fill'} onChange={e=>setStroke(e.target.value==='stroke')}><option value="fill">{tr('칠')}</option><option value="stroke">{tr('획')}</option></select></label>
    {gradient?<><div>{gradient.name}</div><div style={{height:20,background:ramp(gradient.stops??defaultStops,ctx.info!.swatches)}}/>
      {ctx.panelOptions?.gradient!==false&&<>
      {(['x','y'] as const).map((axis,i)=><label key={axis}>{tr('시작점 {0}',{0:axis.toUpperCase()})}<PreviewNumberField value={geo?.start[axis]??0} label={tr('시작점 {0}',{0:axis.toUpperCase()})} undoLabel={tr('그레이디언트 위치')}
        min={-1e6} max={1e6} step={1} decimals={4} width={72} disabled={!enabled} isolateKeys edit={ctx.edit}
        build={v=>[{op:'setGradientGeometry',id:selected!.item.id,stroke,start:i===0?[v,geo?.start.y??0]:[geo?.start.x??0,v]}]}/></label>)}
      <label>{tr('각도')}<PreviewNumberField value={geo?.angle??0} label={tr('그레이디언트 각도')} undoLabel={tr('그레이디언트 위치')} min={-180} max={180} step={1} decimals={4} width={72} disabled={!enabled} isolateKeys edit={ctx.edit}
        build={v=>[{op:'setGradientGeometry',id:selected!.item.id,stroke,angle:v}]}/></label>
      <label>{tr('길이')}<PreviewNumberField value={geo?.length??0} label={tr('그레이디언트 길이')} undoLabel={tr('그레이디언트 위치')} min={0} max={1e6} step={1} decimals={4} width={72} disabled={!enabled} isolateKeys edit={ctx.edit}
        build={v=>[{op:'setGradientGeometry',id:selected!.item.id,stroke,length:v}]}/></label>
      <GradientEditor ctx={ctx} gradient={gradient} pick={pickedStop}/>
      <button disabled={!enabled} onClick={()=>ctx.colorMenu?.openGradient(gradient.id)}>{tr('그레이디언트 견본 옵션')}</button>
      </>}
    </>:<p>{tr('그레이디언트를 적용한 개체를 선택하세요.')}</p>}
  </div>;
}
