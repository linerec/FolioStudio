import { useEffect, useState } from 'react';

export const STYLE_OVERRIDES_KEY = 'viewPreferences.styleOverrideHighlighter';
type Mark = {page:number;quad:[number,number][];paragraph:boolean;character:boolean};
export function useStyleOverrideMarks(enabled:boolean, document:string, revision:Map<number,number>) {
  const [marks,setMarks]=useState<Mark[]>([]);
  useEffect(()=>{
    let active=true;setMarks([]);
    if(enabled)void window.folio.request({cmd:'styleOverrideMarks'}).then(({header})=>{if(active&&header.ok)setMarks(header.marks as Mark[]);}).catch(()=>{});
    return()=>{active=false;};
  },[enabled,document,revision]);
  return (page:number,width:number,height:number)=>enabled?<svg data-style-overrides="true" viewBox={`0 0 ${width} ${height}`} style={{position:'absolute',inset:0,width:'100%',height:'100%',pointerEvents:'none'}}>
    {marks.filter(m=>m.page===page).map((m,i)=><polygon key={i} points={m.quad.map(p=>p.join(',')).join(' ')} fill="#00b9e8" fillOpacity={0.28}/>)}</svg>:null;
}
