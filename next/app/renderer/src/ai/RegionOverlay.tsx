import {useRef,useState,type PointerEvent} from 'react';
import {askDocument} from './attachments';
import type {Target} from '../print';

type Point=[number,number];
const bounds=(a:Point,b:Point)=>[Math.min(a[0],b[0]),Math.min(a[1],b[1]),Math.max(a[0],b[0]),Math.max(a[1],b[1])];
export function RegionOverlay({page,width,height,active,targets}:{page:number;width:number;height:number;active:boolean;targets:Target[]}) {
  const start=useRef<{id:number;at:Point}|null>(null);
  const [preview,setPreview]=useState<number[]>();
  const boxes=targets.filter(t=>t.page===page).map(t=>t.bounds??[0,0,width,height]);
  if(!active && !boxes.length)return null;
  function position(e:PointerEvent<SVGSVGElement>):Point {
    const p=new DOMPoint(e.clientX,e.clientY).matrixTransform(e.currentTarget.getScreenCTM()!.inverse());
    return [Math.max(0,Math.min(width,p.x)),Math.max(0,Math.min(height,p.y))];
  }
  function cancel(){start.current=null;setPreview(undefined);}
  return <svg className="ai-page-overlay" data-ai-region={active||undefined} viewBox={`0 0 ${width} ${height}`}
    style={{position:'absolute',inset:0,width:'100%',height:'100%',zIndex:12,pointerEvents:active?'auto':'none',touchAction:'none',cursor:active?'crosshair':undefined}}
    tabIndex={active?-1:undefined}
    onPointerDown={e=>{
      if(!active||e.button!==0||start.current)return;
      e.preventDefault();e.stopPropagation();e.currentTarget.focus({preventScroll:true});
      start.current={id:e.pointerId,at:position(e)};
      if(e.currentTarget.hasPointerCapture(e.pointerId) || e.isTrusted)e.currentTarget.setPointerCapture(e.pointerId);
    }}
    onPointerMove={e=>{if(start.current?.id===e.pointerId){e.stopPropagation();setPreview(bounds(start.current.at,position(e)));}}}
    onPointerUp={e=>{
      if(start.current?.id!==e.pointerId)return;
      e.stopPropagation();
      const region=bounds(start.current.at,position(e));
      cancel();
      if(region[2]>region[0] && region[3]>region[1])askDocument([{kind:'region',page,region}],false,!e.shiftKey);
    }}
    onLostPointerCapture={cancel} onPointerCancel={cancel}>
    {[...boxes,...(active && preview?[preview]:[])].map(([x,y,right,bottom],i)=>
      <rect key={i} x={x} y={y} width={right-x} height={bottom-y} fill="var(--accent)" fillOpacity=".09" stroke="var(--accent)" strokeWidth="2" vectorEffect="non-scaling-stroke"/>)}
  </svg>;
}
