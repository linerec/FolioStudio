// What InDesign 2025 draws on the layout for the PDF comments tied to it (shown or hidden by the eye button of the PDF Comments panel).
// Measured 2026-11-11 and 2026-11-12 (artifacts/pdfcomments-c4-20261023, -c5-20261023, rules/pdfcomments.ko.md, pdfcomments-c5.ko.md): a markup on
// text is drawn over the commented characters — highlight: yellow (255,234,85) behind the text, strikethrough: a red (255,0,0) line a little below
// the middle of the line, underline: a green (104,217,38) line along the bottom of the line, squiggly: a green wave; a note, a text box, a typewriter
// text, a stamp and a callout get a badge, a yellow (222,190,0) rounded square with a white glyph at the top left corner of the comment's box (a
// callout's at the start of its leader line), 26 interface pixels wide whatever the zoom (39 px on the measured 150 % screen). Line, arrow, oval,
// rectangle, polygon, connected-lines, pencil and cloud comments are NOT drawn at all: the engine sends no mark for them.
import {useEffect,useState} from 'react';
import {type PdfCommentMark,type PdfCommentSelection,readPdfCommentMarks} from './query';

type Props={page:number;width:number;height:number;scale:number;rev:number;selected?:string|null};
const BADGE=26;

/** The glyph in the badge (measured: the note's speech bubble, the text box's frame, the typewriter's T, the callout's text with an arrow, the stamp). */
function Glyph({kind}:{kind:string}) {
  switch(kind){
    case 'StickyNoteType':return <path d="M6.5 7h13a1.5 1.5 0 0 1 1.5 1.5v7a1.5 1.5 0 0 1-1.5 1.5H13l-4 3.2V17H6.5A1.5 1.5 0 0 1 5 15.5v-7A1.5 1.5 0 0 1 6.5 7z" fill="#fff"/>;
    case 'TextTypewriterType':return <g fill="none" stroke="#fff" strokeWidth="1.6"><path d="M6 6h14v14H6z"/><path d="M9.5 10h7M13 10v7"/></g>;
    case 'TextCalloutType':return <g fill="#fff"><path d="M5 6h12v10H5z"/><path d="M15 15l5.5-5.5M16.5 9.5h4v4z" stroke="#fff" strokeWidth="1.4"/><path d="M7 9h8M7 12h6" stroke="rgb(222,190,0)" strokeWidth="1.2"/></g>;
    case 'StampType':return <path d="M11 5h4v6l3 1.5V15H8v-2.5L11 11zM7 17h12v2.5H7z" fill="#fff"/>;
    default:return <path d="M7 7h12v12H7z M10 10h6v6h-6z" fill="none" stroke="#fff" strokeWidth="1.6"/>;
  }
}
function Badge({at,scale,kind}:{at:[number,number];scale:number;kind:string}) {
  const u=BADGE/scale/26;
  return <g transform={`translate(${at[0]} ${at[1]}) scale(${u})`} className="pdfcomment-badge">
    <rect width="26" height="26" rx="4" fill="rgb(222,190,0)"/>
    <Glyph kind={kind}/>
  </g>;
}

// The comment picked in the panel (measured 2026-11-12, rules/pdfcomments-c5.ko.md section 5): a badge gets a blue (0,118,207) square frame 2 interface pixels
// thick whose outer edge is 3.33 px outside the badge; a comment InDesign draws nothing for gets a blue frame mark 20.67 px wide (5.33 px thick, white inside)
// in the middle of its page.
const BLUE='rgb(0,118,207)';
function Selection({sel,scale}:{sel:PdfCommentSelection;scale:number}) {
  const u=1/scale;
  if(sel.ring)return <g transform={`translate(${sel.ring[0]} ${sel.ring[1]}) scale(${u})`} className="pdfcomment-selection" data-kind={sel.kind} data-shape="ring">
    <rect x={-2.33} y={-2.33} width={30.67} height={30.67} fill="none" stroke={BLUE} strokeWidth={2}/>
  </g>;
  if(sel.center)return <g transform={`translate(${sel.center[0]} ${sel.center[1]}) scale(${u})`} className="pdfcomment-selection" data-kind={sel.kind} data-shape="frame">
    <rect x={-7.67} y={-7.67} width={15.33} height={15.33} fill="none" stroke={BLUE} strokeWidth={5.33}/>
  </g>;
  return null;
}

export function PdfCommentMarks({page,width,height,scale,rev,selected}:Props) {
  const [shown,setShown]=useState<{marks:PdfCommentMark[];selection:PdfCommentSelection|null}>({marks:[],selection:null});
  useEffect(()=>{
    let live=true;
    readPdfCommentMarks(page,selected).then(m=>{if(live)setShown(m);},()=>{if(live)setShown({marks:[],selection:null});});
    return ()=>{live=false;};
  },[page,rev,selected]);
  const {marks,selection}=shown;
  if(!marks.length&&!selection)return null;
  return <svg className="pdfcomment-marks" viewBox={`0 0 ${width} ${height}`} aria-hidden="true"
    style={{position:'absolute',inset:0,width:'100%',height:'100%',pointerEvents:'none',overflow:'visible'}}>
    {marks.map(m=><g key={m.id} data-kind={m.kind}>
      {m.boxes?.map((b,i)=>{
        const [from,to,up,down]=b,dx=to[0]-from[0],dy=to[1]-from[1];
        const line=(t:number,colour:string,weight:number)=>{
          const x=up[0]+(down[0]-up[0])*t,y=up[1]+(down[1]-up[1])*t;
          return <line x1={x} y1={y} x2={x+dx} y2={y+dy} stroke={colour} strokeWidth={weight} vectorEffect="non-scaling-stroke"/>;
        };
        if(m.kind==='HighlightTextType')
          return <polygon key={i} points={`${up[0]},${up[1]} ${up[0]+dx},${up[1]+dy} ${down[0]+dx},${down[1]+dy} ${down[0]},${down[1]}`} fill="rgb(255,234,85)" style={{mixBlendMode:'multiply'}}/>;
        if(m.kind==='StrikethroughTextType')return <g key={i}>{line(0.63,'rgb(255,0,0)',2)}</g>;
        if(m.kind==='SquigglyTextType'){
          // measured: a green wave along the bottom of the line
          const len=Math.hypot(dx,dy)||1,ux=dx/len,uy=dy/len,steps=Math.max(2,Math.round(len/2)),a=1;
          const pts=Array.from({length:steps+1},(_,k)=>{const s=Math.min(len,k*len/steps),o=k%2?a:-a;return `${down[0]+ux*s-uy*o},${down[1]+uy*s+ux*o}`;});
          return <polyline key={i} points={pts.join(' ')} fill="none" stroke="rgb(104,217,38)" strokeWidth={1.5} vectorEffect="non-scaling-stroke"/>;
        }
        return <g key={i}>{line(1,'rgb(104,217,38)',1.5)}</g>;
      })}
      {m.badge&&<Badge at={m.badge} scale={scale} kind={m.kind}/>}
    </g>)}
    {selection&&<Selection sel={selection} scale={scale}/>}
  </svg>;
}
