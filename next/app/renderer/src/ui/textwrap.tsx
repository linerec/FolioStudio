import { t as tr } from '../i18n';
import type {EditCommand} from '../engine';
import {PreviewNumberField} from './controls/previewnumberfield';
import type {ItemInfo} from '../inspect';
import type {PanelCtx} from './panels';
import {Icon} from './icons';
// a function: labels follow the current language
const modes=()=>[
  ['NONE','wrapNone',tr('감싸기 없음'),''],
  ['BOUNDING_BOX_TEXT_WRAP','wrapBox',tr('경계 상자'),'BoundingBox'],
  ['CONTOUR','wrapShape',tr('개체 모양'),'Contour'],
  ['JUMP_OBJECT_TEXT_WRAP','wrapJump',tr('개체 건너뛰기'),'JumpObject'],
  ['NEXT_COLUMN_TEXT_WRAP','textWrap',tr('다음 단으로'),'NextColumn'],
];
export function TextWrapControls({ctx,item}:{ctx:PanelCtx;item:ItemInfo|null}) {
  const disabled=!ctx.editable||!item||item.locked||ctx.selection.length!==1;
  const w=item?.text_wrap;
  const apply=(props:{mode?:string;inverse?:boolean;offsets?:[number,number,number,number];side?:string})=>{
    if(!disabled)ctx.edit(tr('텍스트 감싸기'),[{op:'textWrap',id:item!.id,...props}]);
  };
  return <div className="p-form">
    <span className="seg">{modes().map(([mode,icon,label,model])=><button key={mode} title={label} aria-label={label}
      disabled={disabled} className={`${(w?.mode??'')===model?'on':''}`} aria-pressed={(w?.mode??'')===model} onClick={()=>apply({mode})}><Icon name={icon} size={16}/></button>)}</span>
    <label className="p-check"><input type="checkbox" checked={w?.inverse??false} disabled={disabled||!w||!['BoundingBox','Contour'].includes(w.mode)}
      onChange={e=>apply({inverse:e.target.checked})}/>{tr('반전')}</label>
    {[tr('위쪽'),tr('왼쪽'),tr('아래쪽'),tr('오른쪽')].map((label,i)=><label className="p-row" key={label}>{label} {tr('간격')}
      {/* Shared number field: arrows, buttons, wheel, preview and Esc. */}
      <PreviewNumberField isolateKeys className="muted" title={tr('미완성 — 일부 간격의 인쇄 대조 불일치')} value={w?.offsets[i]??0} min={-10000} max={10000} step={1} width={75}
        label={tr('{0} 감싸기 간격', { 0: label })} undoLabel={tr('텍스트 감싸기')} disabled={disabled||!w} edit={(l,c)=>{if(c.length)void ctx.edit(l,c)}}
        build={(n,base)=>{
          if(base==null&&n===(w?.offsets[i]??0))return [];
          const offsets:[number,number,number,number]=[...(w?.offsets??[0,0,0,0])];offsets[i]=n;
          return [{op:'textWrap',id:item!.id,offsets}] as EditCommand[];
        }}/> pt</label>)}
    {ctx.panelOptions?.textWrap!==false&&<WrapOptions side={w?.side??'BothSides'} disabled={disabled||!w} change={(side)=>apply({side:SIDES().find(([,model])=>model===side)?.[0]})}/>}
  </div>;
}
// Show Options: Wrap To (the side text may run on) and the contour options (not built yet)
const SIDES=():[string,string,string][]=>[
  ['BOTH_SIDES','BothSides',tr('양쪽 모두')],['LEFT_SIDE','LeftSide',tr('왼쪽 면')],['RIGHT_SIDE','RightSide',tr('오른쪽 면')],
  ['SIDE_TOWARDS_SPINE','SideTowardsSpine',tr('책등 쪽')],['SIDE_AWAY_FROM_SPINE','SideAwayFromSpine',tr('책등 반대쪽')],['LARGEST_AREA','LargestArea',tr('가장 큰 영역')],
];
function WrapOptions({side,disabled,change}:{side:string;disabled:boolean;change:(side:string)=>void}) {
  return <div className="wrap-options">
    <div className="p-sep"/>
    <label className="p-row">{tr('감싸기 대상:')} <select aria-label={tr('감싸기 대상')} value={side} disabled={disabled} onChange={e=>change(e.target.value)}>
      {SIDES().map(([dom,model,label])=><option key={dom} value={model}>{label}</option>)}</select></label>
    <div className="al-title">{tr('윤곽선 옵션')}</div>
    <label className="p-row">{tr('유형:')} <select className="muted" aria-label={tr('윤곽선 유형')} title={tr('지원하지 않는 옵션입니다.')} disabled><option>{tr('윤곽선 경계 상자')}</option></select></label>
    <label className="p-check" title={tr('지원하지 않는 옵션입니다.')}><input type="checkbox" disabled/>{tr('안쪽 가장자리 포함')}</label>
  </div>;
}
