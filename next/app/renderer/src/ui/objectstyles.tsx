import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { getObjectStyleDefaults, getObjectStyleInfo, type ObjectStyleInfo, type EditCommand } from '../engine';
import { findItem } from './overlay';
import { styleTree, useStoredFlag } from './stylerows';
import { NameDialog } from './tablestylegroups';
import { GroupPickDialog, groupIdOf } from './objectstylegroups';
import { GroupDeleteDialog } from './textstyles/groupdelete';
import { LoadStylesDialog } from './loadstyles';
import { clearStyleOverrides, fieldKey } from './keys';
import type { PanelCtx } from './panels';
import { StyleRowMenu, useRowMenu } from './stylecontext';
import { objectStyleRowActions } from './objectstylemenu';
import { NumberField } from './controls/numberfield';
import './controls/numberfield.dialog.css';
import './objectstyles.css';

type Mode='create'|'edit'|'duplicate'|'delete';
type Field={key:string;label:string;type?:'boolean'|'number'|'color'|'corner'|'wrap'|'paragraph'};
const categories=():{label:string;fields:Field[]}[]=>[
  {label:tr('칠'),fields:[{key:'enableFill',label:tr('사용'),type:'boolean'},{key:'fillColor',label:tr('칠 색상'),type:'color'},{key:'fillTint',label:tr('농도'),type:'number'}]},
  {label:tr('획'),fields:[{key:'enableStroke',label:tr('사용'),type:'boolean'},{key:'strokeColor',label:tr('획 색상'),type:'color'},{key:'strokeWeight',label:tr('두께'),type:'number'},{key:'strokeTint',label:tr('농도'),type:'number'}]},
  {label:tr('모퉁이'),fields:[{key:'enableStrokeAndCornerOptions',label:tr('사용'),type:'boolean'},...['topLeft','topRight','bottomLeft','bottomRight'].flatMap((key,i)=>[
    {key:key+'CornerOption',label:[tr('왼쪽 위'),tr('오른쪽 위'),tr('왼쪽 아래'),tr('오른쪽 아래')][i],type:'corner' as const},
    {key:key+'CornerRadius',label:tr('반경'),type:'number' as const}])]},
  {label:tr('투명도'),fields:[{key:'objectEffectsEnablingSettings.enableTransparency',label:tr('사용'),type:'boolean'},{key:'transparencySettings.blendingSettings.opacity',label:tr('불투명도'),type:'number'}]},
  {label:tr('텍스트 프레임'),fields:[{key:'enableTextFrameGeneralOptions',label:tr('사용'),type:'boolean'},{key:'textFramePreferences.textColumnCount',label:tr('단 수'),type:'number'},{key:'textFramePreferences.textColumnGutter',label:tr('단 간격'),type:'number'}]},
  {label:tr('텍스트 감싸기'),fields:[{key:'enableTextWrapAndOthers',label:tr('사용'),type:'boolean'},{key:'textWrapPreferences.textWrapMode',label:tr('종류'),type:'wrap'}]},
  {label:tr('문단 스타일'),fields:[{key:'enableParagraphStyle',label:tr('사용'),type:'boolean'},{key:'appliedParagraphStyle',label:tr('문단 스타일'),type:'paragraph'},{key:'applyNextParagraphStyle',label:tr('다음 스타일 적용'),type:'boolean'}]},
  {label:tr('외부 광선'),fields:[{key:'objectEffectsEnablingSettings.enableOuterGlow',label:tr('사용'),type:'boolean'}]},
  {label:tr('내부 그림자'),fields:[{key:'objectEffectsEnablingSettings.enableInnerShadow',label:tr('사용'),type:'boolean'}]},
  {label:tr('그림자'),fields:[{key:'objectEffectsEnablingSettings.enableDropShadow',label:tr('사용'),type:'boolean'}]},
  {label:tr('내부 광선'),fields:[{key:'objectEffectsEnablingSettings.enableInnerGlow',label:tr('사용'),type:'boolean'}]},
  {label:tr('경사와 엠보스'),fields:[{key:'objectEffectsEnablingSettings.enableBevelEmboss',label:tr('사용'),type:'boolean'}]},
  {label:tr('새틴'),fields:[{key:'objectEffectsEnablingSettings.enableSatin',label:tr('사용'),type:'boolean'}]},
  {label:tr('기본 페더'),fields:[{key:'objectEffectsEnablingSettings.enableFeather',label:tr('사용'),type:'boolean'}]},
  {label:tr('방향 페더'),fields:[{key:'objectEffectsEnablingSettings.enableDirectionalFeather',label:tr('사용'),type:'boolean'}]},
  {label:tr('그레이디언트 페더'),fields:[{key:'objectEffectsEnablingSettings.enableGradientFeather',label:tr('사용'),type:'boolean'}]},
];
/** A style property as a number for a number box: a number, or a numeric text; blank (null) when it has none. */
function numberOf(v:unknown):number|null {if(typeof v==='number')return Number.isFinite(v)?v:null;if(typeof v==='string'&&v.trim()!==''&&Number.isFinite(Number(v)))return Number(v);return null;}
function get(props:Record<string,unknown>,key:string):unknown {return key.split('.').reduce<unknown>((v,k)=>(v&&typeof v==='object')?(v as Record<string,unknown>)[k]:undefined,props);}
function put(props:Record<string,unknown>,key:string,value:unknown) {const path=key.split('.');let p=props;for(const k of path.slice(0,-1)){p[k]={...p[k] as object};p=p[k] as Record<string,unknown>;}p[path.at(-1)!]=value;}

export function ObjectStylesPanel({ctx}:{ctx:PanelCtx}) {
  const [,tick]=useState(0);useEffect(()=>ctx.store.subscribe(()=>tick(n=>n+1)),[ctx.store]);
  const list=ctx.info?.objectStyles??[];
  const targets=ctx.selection.map(e=>findItem(ctx.store.peek(e.page),e.id)?.item).filter(it=>it&&!it.locked);
  const hasContent=ctx.selection.some(e=>e.content);
  const ids=targets.map(it=>it!.id),canApply=ctx.editable&&ids.length>0&&ids.length===ctx.selection.length;
  const canCapture=canApply&&!hasContent&&ids.length===1&&['rectangle','oval','polygon','line','textFrame'].includes(targets[0]!.kind);
  const canClearUndefined=canCapture&&targets[0]!.kind!=='textFrame';
  const applied=canApply&&!hasContent&&targets.every(it=>it!.style===targets[0]!.style)?targets[0]!.style:null;
  // the panel menu: small rows and "clear overrides when applying" are this computer's settings; the open groups are only on screen
  const [small,setSmall]=useStoredFlag('folio.objectStyles.smallRows'),[clearOnApply,setClearOnApply]=useStoredFlag('folio.objectStyles.clearOverridesWhenApplying');
  const [closed,setClosed]=useState<Set<string>>(()=>new Set());
  // Default Text Frame Style / Default Graphic Frame Style: the document's (its preferences), read again after every change
  const [defaults,setDefaults]=useState<{text:string|null;graphic:string|null}>({text:null,graphic:null});
  useEffect(()=>{let live=true;getObjectStyleDefaults().then(r=>{if(live)setDefaults(r.style);},()=>{});return()=>{live=false;};},[ctx.info]);
  const groupInfos=ctx.info?.objectStyleGroups??[];
  const tree=styleTree(list,closed,groupInfos.map(g=>g.path));
  // the group picked in the list (its path joined) and the small windows of the group commands
  // the styles chosen together (Select All Unused); Delete Styles… takes them all
  const [picked,setPicked]=useState<string[]>([]);
  const [pickedGroup,setPickedGroup]=useState(''),[sub,setSub]=useState<''|'newGroup'|'copyTo'|'fromStyles'|'load'|'renameGroup'|'deleteGroup'>('');
  const groupPath=pickedGroup?pickedGroup.split('/'):[],groupInfo=groupPath.length?groupInfos.find(g=>g.path.join('/')===pickedGroup)??null:null;
  const [selected,setSelected]=useState(''),[dialog,setDialog]=useState<{mode:Mode;info:ObjectStyleInfo}|null>(null),[error,setError]=useState('');
  const id=selected||applied||list[0]?.id||'';
  const ready=list.some(s=>s.id===id);
  const protectedStyle=id.includes('/$ID/');
  const open=async(mode:Mode,chosen=id)=>{setError('');try{const source=mode==='create'?(applied??'ObjectStyle/$ID/[Normal Graphics Frame]'):chosen;const r=await getObjectStyleInfo(source);setDialog({mode,info:r.style});}catch(e){setError(String(e));}};
  // Apply Style on the selected objects: `clear` takes off what the objects set themselves, `undefinedToo` also what the style does not define
  const applyStyle=(style:string,clear:boolean,undefinedToo:boolean)=>{
    if(!canApply)return;
    const apply:EditCommand[]=ctx.selection.map(target=>({op:target.content?'applyObjectStyleToContent':'applyObjectStyle',ids:[target.id],style,clear}));
    ctx.edit(tr('개체 스타일 적용'),undefinedToo?[...apply,{op:'clearOverridesNotInStyle',ids}]:apply);
  };
  const actions=[
    {id:'new',label:canCapture?tr('선택에서 새 스타일'):tr('새 개체 스타일'),disabled:!ctx.editable||(ctx.selection.length>0&&!canCapture),run:()=>open('create')},
    {id:'options',label:tr('옵션'),disabled:!ctx.editable||!ready||protectedStyle,run:()=>open('edit')},
    {id:'duplicate',label:tr('복제'),disabled:!ctx.editable||!ready||protectedStyle||id.includes('%3a'),run:()=>open('duplicate')},
    {id:'delete',label:tr('삭제'),disabled:!ctx.editable||!ready||protectedStyle,run:()=>open('delete')},
    {id:'redefine',label:tr('다시 정의'),disabled:!canCapture||protectedStyle||applied!==id,run:()=>ctx.edit(tr('스타일 다시 정의'),[{op:'redefineObjectStyle',style:id,from:ids[0]}])},
    {id:'clear',label:tr('재정의 지우기'),disabled:!canApply||hasContent,run:()=>ctx.edit(tr('재정의 지우기'),[{op:'clearObjectStyleOverrides',ids}])},
    {id:'clearUndefined',label:tr('스타일에 없는 속성 지우기'),disabled:!canClearUndefined,run:()=>ctx.edit(tr('스타일에 없는 속성 지우기'),[{op:'clearOverridesNotInStyle',ids}])},
    // the rest are in the panel menu only
    {id:'clearOnApply',label:tr('스타일 적용 시 재정의 지우기'),disabled:false,checked:clearOnApply,menuOnly:true,run:()=>setClearOnApply(!clearOnApply)},
    {id:'newGroup',label:tr('새 스타일 그룹…'),disabled:!ctx.editable,menuOnly:true,run:()=>setSub('newGroup')},
    {id:'copyToGroup',label:tr('그룹에 복사…'),disabled:!ctx.editable||!ready||id.includes('/$ID/')||groupInfos.length===0,menuOnly:true,run:()=>setSub('copyTo')},
    {id:'newGroupFromStyles',label:tr('스타일에서 새 그룹…'),disabled:!ctx.editable||!ready||id.includes('/$ID/'),menuOnly:true,run:()=>setSub('fromStyles')},
    ...(['text','graphic'] as const).flatMap(kind=>(['ObjectStyle/$ID/[None]','ObjectStyle/$ID/[Normal Graphics Frame]','ObjectStyle/$ID/[Normal Text Frame]'] as const).map((style,i)=>
      ({id:`default.${kind}.${['none','basicGraphicsFrame','basicTextFrame'][i]}`,label:style,disabled:!ctx.editable,checked:defaults[kind]===style,menuOnly:true,
        run:()=>ctx.edit(tr('기본 프레임 스타일'),[{op:'setDefaultFrameStyle',kind,style}])}))),
    {id:'breakLink',label:tr('스타일에 대한 연결 끊기'),disabled:!canApply||!applied||applied==='ObjectStyle/$ID/[None]',menuOnly:true,run:()=>ctx.edit(tr('스타일에 대한 연결 끊기'),[{op:'breakLinkToStyle',ids}])},
    {id:'selectUnused',label:tr('사용하지 않는 항목 모두 선택'),disabled:!list.some(s=>!s.used&&!s.id.includes('/$ID/')),menuOnly:true,
      run:()=>{const unused=list.filter(s=>!s.used&&!s.id.includes('/$ID/')).map(s=>s.id);setPicked(unused);setSelected(list.find(s=>unused.includes(s.id)&&!s.group.length)?.id??unused[0]??'');}},
    {id:'load',label:tr('개체 스타일 불러오기…'),disabled:!ctx.editable,menuOnly:true,run:()=>setSub('load')},
    {id:'sort',label:tr('이름순 정렬'),disabled:!ctx.editable||list.length<2,menuOnly:true,run:()=>ctx.edit(tr('이름순 정렬'),[{op:'sortObjectStyles'}])},
    {id:'openAll',label:tr('모든 그룹 열기'),disabled:tree.groups.length===0,menuOnly:true,run:()=>setClosed(new Set())},
    {id:'closeAll',label:tr('모든 그룹 닫기'),disabled:tree.groups.length===0,menuOnly:true,run:()=>setClosed(new Set(tree.groups))},
    {id:'small',label:tr('작은 패널 행'),disabled:false,checked:small,menuOnly:true,run:()=>setSmall(!small)},
    // the row's right-click menu: Apply Style, and with it clearing the overrides, the attributes the style does not define, or both
    // the group rows' right-click menu: Style Group Options… (rename), Duplicate Group, Delete Group… (the engine's renameObjectStyleGroup, duplicateStyleGroup, deleteObjectStyleGroup)
    {id:'groupOptions',label:'',disabled:!ctx.editable||!groupInfo,menuOnly:true,run:()=>setSub('renameGroup')},
    {id:'duplicateGroup',label:'',disabled:!ctx.editable||!groupInfo,menuOnly:true,run:()=>ctx.edit(tr('스타일 그룹 복제'),[{op:'duplicateStyleGroup',kind:'object',group:groupPath.join(':')}])},
    {id:'deleteGroup',label:'',disabled:!ctx.editable||!groupInfo,menuOnly:true,run:()=>setSub('deleteGroup')},
    {id:'apply',label:'',disabled:!canApply||!ready,menuOnly:true,run:()=>applyStyle(id,false,false)},
    {id:'applyClear',label:'',disabled:!canApply||!ready,menuOnly:true,run:()=>applyStyle(id,true,false)},
    {id:'applyUndefined',label:'',disabled:!canClearUndefined||!ready,menuOnly:true,run:()=>applyStyle(id,false,true)},
    {id:'applyAll',label:'',disabled:!canClearUndefined||!ready,menuOnly:true,run:()=>applyStyle(id,true,true)},
  ];
  const rowMenu=useRowMenu();
  useEffect(()=>{ctx.objectStyleActions?.(actions);});
  useEffect(()=>()=>ctx.objectStyleActions?.([]),[ctx.objectStyleActions]);
  return <div className="styles-panel" data-object-styles-panel>
    <div className={small?'st-list small-rows':'st-list'} style={{minHeight:110}}>{tree.rows.map(r=>r.group?
      <button key={'g:'+r.key} className="st-row" data-style-group={r.key} aria-expanded={r.open} aria-pressed={pickedGroup===r.key} style={{paddingLeft:10+r.depth*14}}
        onContextMenu={e=>{setPickedGroup(r.key);setPicked([]);rowMenu.open(e);}}
        onClick={()=>{setPickedGroup(r.key);setClosed(c=>{const n=new Set(c);if(n.has(r.key))n.delete(r.key);else n.add(r.key);return n;});}}>{(r.open?'▾ ':'▸ ')+r.name}</button>:
      <button key={r.style.id} className="st-row" data-object-style={r.style.id} aria-pressed={id===r.style.id||picked.includes(r.style.id)} disabled={!ctx.editable} style={{paddingLeft:10+r.depth*14}}
      onContextMenu={e=>{setSelected(r.style.id);setPicked([]);setPickedGroup('');rowMenu.open(e);}}
      onClick={e=>{setSelected(r.style.id);setPicked([]);setPickedGroup('');if(canApply)ctx.edit(tr('개체 스타일 적용'),ctx.selection.map(target=>({op:target.content?'applyObjectStyleToContent':'applyObjectStyle',ids:[target.id],style:r.style.id,clear:clearOnApply||clearStyleOverrides(e)})));}}
      onDoubleClick={()=>{setSelected(r.style.id);if(!r.style.id.includes('/$ID/'))open('edit',r.style.id);}}>{r.name}{applied===r.style.id?' ✓':''}</button>)}</div>
    <div className="p-foot style-actions">{actions.filter(a=>!a.menuOnly).map(a=><button key={a.id} data-os-action={a.id} disabled={a.disabled} onClick={a.run}>{a.label}</button>)}</div>
    {error&&<p role="alert">{error}</p>}
    <StyleRowMenu menu="styleObject" at={rowMenu.at} commands={ctx.commands} actions={objectStyleRowActions(actions.map(a=>pickedGroup?(['groupOptions','duplicateGroup','deleteGroup'].includes(a.id)?a:{...a,disabled:true}):a))} name={pickedGroup?groupPath.join(' / '):(list.find(s=>s.id===id)?.name??'')} close={rowMenu.close}/>
    {sub==='newGroup'&&<NameDialog title={tr('새 스타일 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={n=>groupInfos.some(g=>g.path.join('/')===[...(pickedGroup?pickedGroup.split('/'):[]),n].join('/'))} close={()=>setSub('')}
      onOk={n=>ctx.edit(tr('새 스타일 그룹'),[{op:'createObjectStyleGroup',name:n,...(pickedGroup?{parent:groupIdOf(pickedGroup.split('/'),groupInfos)}:{})}])}/>}
    {sub==='fromStyles'&&<NameDialog title={tr('스타일에서 새 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={n=>groupInfos.some(g=>g.path.join('/')===n)} close={()=>setSub('')}
      onOk={n=>ctx.edit(tr('스타일에서 새 그룹'),[{op:'createObjectStyleGroup',name:n},{op:'moveObjectStyle',style:id,group:groupIdOf([n],[])}])}/>}
    {sub==='renameGroup'&&groupInfo&&<NameDialog title={tr('스타일 그룹 이름 바꾸기')} label={tr('이름:')} initial={groupPath[groupPath.length-1]} close={()=>setSub('')}
      taken={n=>n!==groupPath[groupPath.length-1]&&groupInfos.some(g=>g.path.join('/')===[...groupPath.slice(0,-1),n].join('/'))}
      onOk={n=>{ctx.edit(tr('스타일 그룹 이름 바꾸기'),[{op:'renameObjectStyleGroup',group:groupInfo.id,name:n}]);setPickedGroup([...groupPath.slice(0,-1),n].join('/'));}}/>}
    {sub==='deleteGroup'&&groupInfo&&<GroupDeleteDialog name={groupPath.join(' / ')} count={list.filter(s=>s.group.length>=groupPath.length&&groupPath.every((n,i)=>s.group[i]===n)).length}
      onOk={()=>{ctx.edit(tr('스타일 그룹 삭제'),[{op:'deleteObjectStyleGroup',group:groupInfo.id}]);setPickedGroup('');}} close={()=>setSub('')}/>}
    {sub==='load'&&<LoadStylesDialog ctx={ctx} kinds="object" close={()=>setSub('')}/>}
    {sub==='copyTo'&&<GroupPickDialog title={tr('그룹에 복사')} groups={groupInfos} close={()=>setSub('')} onOk={g=>ctx.edit(tr('그룹에 복사'),[{op:'copyObjectStyleToGroup',style:id,group:g}])}/>}
    {dialog&&<ObjectStyleDialog ctx={ctx} {...dialog} together={picked.length>1&&picked.includes(dialog.info.id)?picked:undefined} source={canCapture?ids[0]:undefined} choose={setSelected} close={()=>setDialog(null)}/>}
  </div>;
}

function ObjectStyleDialog({ctx,mode,info,source,together,choose,close}:{ctx:PanelCtx;mode:Mode;info:ObjectStyleInfo;source?:string;together?:string[];choose:(id:string)=>void;close:()=>void}) {
  const list=ctx.info?.objectStyles??[];
  const unique=(stem:string)=>{let n=stem;for(let i=2;list.some(s=>s.name===n);i++)n=`${stem} ${i}`;return n;};
  const leaf=list.find(s=>s.id===info.id)?.name??info.name;
  const [name,setName]=useState(mode==='create'?unique(tr('새 개체 스타일')):mode==='duplicate'?unique(tr('{0} 복사본',{0:leaf})):leaf);
  const [values,setValues]=useState(info.props),[dirty,setDirty]=useState<Record<string,unknown>>({}),[category,setCategory]=useState(0),[error,setError]=useState('');
  const going=together??[info.id];
  const [replacement,setReplacement]=useState(list.find(s=>!going.includes(s.id))?.id??'');
  const textSource=source&&ctx.selection.some(e=>e.id===source&&findItem(ctx.store.peek(e.page),e.id)?.item.kind==='textFrame');
  const [basedOn,setBasedOn]=useState(mode==='edit'?info.basedOn:textSource?info.id:'ObjectStyle/$ID/[None]');
  const set=(key:string,value:unknown)=>{setValues(p=>{const n=structuredClone(p);put(n,key,value);return n;});setDirty(p=>{const n=structuredClone(p);put(n,key,value);return n;});};
  const submit=()=>{
    setError('');const n=name.trim();
    if(mode!=='delete'&&(!n||list.some(s=>s.name===n&&(mode!=='edit'||s.id!==info.id)))){setError(tr('겹치지 않는 스타일 이름을 입력하세요.'));return;}
    let command:EditCommand;
    if(mode==='delete'){ctx.edit(tr('개체 스타일 관리'),going.map(style=>({op:'deleteObjectStyle' as const,style,replacement})));choose(replacement);close();return;}
    else if(mode==='duplicate')command={op:'duplicateObjectStyle',style:info.id,name:n};
    else if(mode==='create')command={op:'createObjectStyle',name:n,basedOn,from:source,props:dirty};
    else command={op:'editObjectStyle',style:info.id,rename:n,props:{...dirty,...(basedOn!==info.basedOn?{basedOn}:{})}};
    const group=mode==='edit'?(list.find(s=>s.id===info.id)?.group??[]):[];
    ctx.edit(tr('개체 스타일 관리'),[command]);choose(`ObjectStyle/${[...group,n].join('%3a')}`);close();
  };
  const groups=categories();
  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('개체 스타일 옵션')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel')close();}}>
    <form className="card dialog style-options object-style-options" onSubmit={e=>{e.preventDefault();submit();}}>
      <h2>{mode==='delete'?tr('스타일 삭제'):tr('개체 스타일 옵션')}</h2>
      <label>{tr('이름')}<input aria-label={tr('스타일 이름')} value={name} disabled={mode==='delete'} onChange={e=>setName(e.target.value)} autoFocus/></label>
      {mode==='delete'?<label>{tr('바꿀 스타일')}<select aria-label={tr('바꿀 스타일')} value={replacement} onChange={e=>setReplacement(e.target.value)}>{list.filter(s=>!going.includes(s.id)).map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label>:mode==='duplicate'?null:<>
        <label>{tr('기준 스타일')}<select aria-label={tr('기준 스타일')} value={basedOn} onChange={e=>setBasedOn(e.target.value)}>{list.filter(s=>mode!=='edit'||s.id!==info.id).map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label>
        {mode==='edit'&&<><div className="style-actions">{groups.map((g,i)=><button key={g.label} type="button" aria-pressed={category===i} onClick={()=>setCategory(i)}>{g.label}</button>)}<button type="button" disabled className="muted" title={tr('지원하지 않는 옵션입니다.')}>{tr('기타 범주')}</button></div>
        <div className="style-fields">{groups[category].fields.map(f=><label key={f.key}>{f.label}{f.type==='boolean'?<input type="checkbox" aria-label={f.key} checked={!!get(values,f.key)} onChange={e=>set(f.key,e.target.checked)}/>:f.type==='paragraph'?<select aria-label={f.key} value={String(get(values,f.key)??'ParagraphStyle/$ID/[No paragraph style]')} onChange={e=>set(f.key,e.target.value)}>{ctx.info?.paragraphStyles.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select>:f.type==='color'?<select aria-label={f.key} value={String(get(values,f.key)??'Swatch/None')} onChange={e=>set(f.key,e.target.value)}>{ctx.info?.swatches.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select>:f.type==='corner'||f.type==='wrap'?<select aria-label={f.key} value={String(get(values,f.key)??'None')} onChange={e=>set(f.key,e.target.value)}>{(f.type==='corner'?[['None',tr('없음')],['RoundedCorner',tr('둥근 모퉁이')]]:[['None',tr('없음')],['BoundingBoxTextWrap',tr('경계 상자')]]).map(([v,n])=><option key={v} value={v}>{n}</option>)}</select>:f.type==='number'?<NumberField label={f.key} value={numberOf(get(values,f.key))} onCommit={n=>set(f.key,n)} min={-1e9} max={1e9} step={1} decimals={4} width={220} inDialog/>:<input aria-label={f.key} type="text" value={String(get(values,f.key)??'')} onChange={e=>set(f.key,e.target.value)}/>}</label>)}</div>
        <p className="muted">{tr('효과 세부 설정·다음 문단 스타일·추가 프레임 옵션은 미완성입니다.')}</p></>}
      </>}
      {error&&<p role="alert">{error}</p>}<div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!ctx.editable}>{tr('확인')}</button></div>
    </form>
  </div>,document.body);
}
