import { t as tr } from '../i18n';
import { useEffect, useState, type DragEvent } from 'react';
import { StyleDialog } from './textstyles/dialog';
import { groupKey, groupPaths, textStyleRows } from './textstyles/rows';
import { NameDialog } from './tablestylegroups';
import { GroupDeleteDialog } from './textstyles/groupdelete';
import { GroupPickDialog } from './objectstylegroups';
import { LoadStylesDialog } from './loadstyles';
import { getStyleInfo, type StyleInfo } from '../engine';
import type { PanelCtx } from './panels';
import { type TextStyleAction, textStyleRowActions } from './textstylemenu';
import { StyleRowMenu, useRowMenu } from './stylecontext';

type Kind='paragraph'|'character';
type Mode='create'|'edit'|'delete';
// [No Paragraph Style] and [None]: the DOM refuses every change ("Invalid request on a root style."); the other built-in style, [Basic Paragraph], takes values
const isRootStyle=(id:string)=>id.endsWith('/$ID/[No paragraph style]')||id.endsWith('/$ID/[No character style]');
export function TextStylesPanel({ctx,kind}:{ctx:PanelCtx;kind:Kind}) {
  const list=(kind==='paragraph'?ctx.info?.paragraphStyles:ctx.info?.characterStyles)??[];
  const selection=ctx.textFormatting.selection, applied=String(selection?.format[kind==='paragraph'?'paragraphStyle':'characterStyle']??'');
  const [selected,setSelected]=useState(''),[dialog,setDialog]=useState<{mode:Mode;info:StyleInfo}|null>(null),[error,setError]=useState('');
  const [picked,setPicked]=useState<string[]|null>(null);
  useEffect(()=>{setPicked(null);setSelected('');},[ctx.doc?.path,kind]);
  // the panel menu: the groups shut, and the small rows
  const [closed,setClosed]=useState<Set<string>>(new Set()),[small,setSmall]=useState(false),[sorted,setSorted]=useState(false);
  // the group picked in the list (a new group or a new style goes into it) and the small dialogs of the menu
  const [pickedGroup,setPickedGroup]=useState<string[] | null>(null),[sub,setSub]=useState<'duplicate'|'newGroup'|'groupFromStyles'|'copyTo'|'load'|'loadAll'|'renameGroup'|'deleteGroup'|null>(null);
  // the last row clicked is a group: Delete removes the group (and what is in it), a double click renames it (oracle text-format tsg-*)
  const [groupSel,setGroupSel]=useState(false);
  const id=picked!==null?(picked.length===1?picked[0]:''):(list.some(s=>s.id===selected)?selected:applied);
  const open=async(mode:Mode)=>{
    setError('');
    try {
      const source=mode==='create'?applied:id;
      if(!source)return;
      const {style}=await getStyleInfo(kind,source);
      setDialog({mode,info:style});
    }catch(e){setError(String(e));}
  };
  // the root styles ([No Paragraph Style], [None]) take nothing; [Basic Paragraph] takes values, a base, nested/GREP/line styles and is duplicated (oracle bs-*)
  const rootStyle=isRootStyle(id);
  const locked=!ctx.editable,idProtected=id.includes('/$ID/')||(picked?.length??0)>1,several=(picked?.length??0)>1;
  const tag=kind==='paragraph'?'ParagraphStyle':'CharacterStyle';
  const current=list.find(s=>s.id===id)??null;
  const paths=groupPaths(list,(kind==='paragraph'?ctx.info?.paragraphStyleGroups:ctx.info?.characterStyleGroups)??[]);
  const parent=pickedGroup&&paths.some(p=>groupKey(p)===groupKey(pickedGroup))?pickedGroup:[];
  const styleId=(path:string[],name:string)=>`${tag}/${[...path,name].join('%3a')}`;
  const groupPick=groupSel&&pickedGroup&&paths.some(p=>groupKey(p)===groupKey(pickedGroup))?pickedGroup:null;
  const inGroup=(path:string[])=>list.filter(s=>s.group.length>=path.length&&path.every((n,i)=>s.group[i]===n)&&!s.id.includes('/$ID/'));
  const renameGroup=(name:string)=>{
    if(!groupPick)return;
    const to=[...groupPick.slice(0,-1),name];
    ctx.edit(tr('스타일 그룹 이름 바꾸기'),[{op:'renameTextStyleGroup',kind,group:groupPick.join(':'),name}]);
    setClosed(c=>{const old=groupKey(groupPick);if(!c.has(old))return c;const n=new Set(c);n.delete(old);n.add(groupKey(to));return n;});
    setPickedGroup(to);
  };
  const deleteGroup=()=>{
    if(!groupPick)return;
    ctx.edit(tr('스타일 그룹 삭제'),[{op:'deleteTextStyleGroup',kind,group:groupPick.join(':')}]);
    setPickedGroup(null);setGroupSel(false);setSelected('');
  };
  // Dragging a style onto a group (or onto the empty list for the top level) moves it there: the engine command moveTextStyle
  // (`style.move(AT_END, group)`, oracle tstyle-group-move and tstyle-group-out); the references follow the style's new id.
  const dragType=`folio/text-style/${kind}`;
  const [dropAt,setDropAt]=useState<string|null>(null);
  const accepts=(e:{dataTransfer:DataTransfer})=>!locked&&e.dataTransfer.types.includes(dragType);
  const moveTo=(source:string,path:string[])=>{
    const style=list.find(s=>s.id===source);
    if(!style||style.id.includes('/$ID/')||groupKey(style.group)===groupKey(path))return;
    if(list.some(s=>s.id===styleId(path,style.name))){setError(tr('같은 이름의 스타일이 이미 있습니다.'));return;}
    setError('');
    ctx.edit(tr('스타일을 그룹으로 옮기기'),[{op:'moveTextStyle',kind,style:style.id,group:path.join(':')}]);
    setSelected(styleId(path,style.name));
  };
  const dropOn=(e:DragEvent,path:string[])=>{e.preventDefault();e.stopPropagation();setDropAt(null);moveTo(e.dataTransfer.getData(dragType),path);};
  const overDrop=(e:DragEvent,key:string)=>{if(accepts(e)){e.preventDefault();e.stopPropagation();setDropAt(key);}};
  const duplicate=(name:string)=>{
    if(!current)return;
    ctx.edit(tr('스타일 복제'),[{op:'duplicateTextStyle',kind,style:current.id,name}]);
    setSelected(styleId(current.group,name));
  };
  const newGroup=(name:string)=>{
    ctx.edit(tr('새 스타일 그룹'),[{op:'createTextStyleGroup',kind,name,parent:parent.join(':')}]);
    setPickedGroup([...parent,name]);
  };
  // New Group from Styles: the picked style goes into a group made new at the top (the panel picks one style at a time)
  const groupFromStyles=(name:string)=>{
    if(!current)return;
    ctx.edit(tr('스타일에서 새 그룹'),[{op:'createTextStyleGroup',kind,name,parent:''},{op:'moveTextStyle',kind,style:current.id,group:name}]);
    setSelected(styleId([name],current.name));
  };
  const redefine=()=>ctx.edit(tr('스타일 다시 정의'),[{op:'styleRedefine',kind,style:id,story:selection!.story,start:selection!.start,end:selection!.end}]);
  const overridden=!!selection?.format[kind==='paragraph'?'paragraphOverride':'characterOverride'];
  const actions:TextStyleAction[]=[
    {id:'highlight',disabled:!ctx.styleOverrideHighlighter,checked:ctx.styleOverrideHighlighter?.enabled??false,run:()=>ctx.styleOverrideHighlighter?.toggle()},
    {id:'selectAllUnused',disabled:!ctx.info,run:()=>{const unused=list.filter(s=>s.used===false&&!s.id.includes('/$ID/')).map(s=>s.id);setPicked(unused);setSelected(unused[0]??'');setPickedGroup(null);setClosed(new Set());}},
    {id:'new',disabled:locked||!selection||!applied,run:()=>open('create')},
    {id:'options',disabled:locked||!id||rootStyle||several,run:()=>open('edit')},
    {id:'redefine',disabled:locked||!selection||!id||rootStyle||several,run:redefine},
    {id:'delete',disabled:locked||(groupPick?false:!id||idProtected),run:()=>groupPick?setSub('deleteGroup'):open('delete')},
    // Clear Overrides: the style is applied again with what the text sets itself taken off (the same as Alt+click on the style)
    ...(kind==='paragraph'?[{id:'clear',disabled:locked||!selection||!applied,run:()=>selection&&ctx.textFormatting.style(kind,applied,true)}]:[]),
    // Break Link to Style: the selected text keeps how it looks and no longer follows the style (the style's settings become its own)
    {id:'breakLink',disabled:locked||!selection||!applied||applied.includes('/$ID/')||(kind==='character'&&selection.start===selection.end),
      run:()=>selection&&ctx.edit(tr('스타일에 대한 연결 끊기'),[{op:'breakStyleLink',kind,story:selection.story,start:selection.start,end:selection.end}])},
    {id:'duplicate',disabled:locked||!current||rootStyle||several,run:()=>setSub('duplicate')},
    {id:'newGroup',disabled:locked,run:()=>setSub('newGroup')},
    {id:'newGroupFromStyles',disabled:locked||!current||idProtected,run:()=>setSub('groupFromStyles')},
    // Copy to Group…: the style stays and a copy with its name goes into the group picked (oracle loadstyles ls-copy-*)
    {id:'copyToGroup',disabled:locked||!current||idProtected||paths.length===0,run:()=>setSub('copyTo')},
    // Load Paragraph / Character Styles… and Load All Text Styles…: styles of another document (oracle loadstyles ls-*)
    {id:'load',disabled:locked,run:()=>setSub('load')},
    {id:'loadAll',disabled:locked,run:()=>setSub('loadAll')},
    {id:'sortByName',disabled:false,run:()=>setSorted(true)},
    {id:'openAll',disabled:false,run:()=>setClosed(new Set())},
    {id:'closeAll',disabled:false,run:()=>setClosed(new Set(paths.map(groupKey)))},
    ...(kind==='paragraph'?[{id:'convertLists',disabled:locked||!current||idProtected,run:()=>ctx.edit(tr('글머리 기호 및 번호 매기기를 텍스트로 변환'),[{op:'convertStyleListsToText',style:id,labels:{}}])}]:[]),
    {id:'smallRows',disabled:false,checked:small,run:()=>setSmall(v=>!v)},
    // the row's right-click menu: Apply (the row's click), Apply and Clear Overrides (Alt+click), and the group commands of a group row
    {id:'apply',disabled:locked||!selection||!id||!!groupPick,run:()=>selection&&ctx.textFormatting.style(kind,id,false)},
    {id:'applyClear',disabled:locked||!selection||!id||!!groupPick,run:()=>selection&&ctx.textFormatting.style(kind,id,true)},
    // Apply with clearing the character styles / everything, and then Next Style (the engine's applyParagraphStyleVariant; the paragraph panel only)
    ...(kind==='paragraph'?(['applyClearChars:characterStyles:0','applyClearAll:all:0','applyNext:none:1','applyNextClear:overrides:1','applyNextClearChars:characterStyles:1','applyNextClearAll:all:1'] as const).map(spec=>{
      const [key,clear,next]=spec.split(':');
      return {id:key,disabled:locked||!selection||!id||!!groupPick,run:()=>selection&&ctx.edit(tr('문단 스타일 적용'),[{op:'applyParagraphStyleVariant',story:selection.story,start:selection.start,end:selection.end,style:id,clear,next:next==='1'}])};
    }):[]),
    {id:'duplicateGroup',disabled:locked||!groupPick,run:()=>groupPick&&ctx.edit(tr('스타일 그룹 복제'),[{op:'duplicateStyleGroup',kind,group:groupPick.join(':')}])},
    {id:'groupOptions',disabled:locked||!groupPick,run:()=>setSub('renameGroup')},
    {id:'deleteGroup',disabled:locked||!groupPick,run:()=>setSub('deleteGroup')},
  ];
  useEffect(()=>{ctx.textStyleActions?.(kind,actions);});
  // right-click on a row: the row is picked, then the menu opens (what applies to a style is off on a group row, and the group commands are off on a style row)
  const rowMenu=useRowMenu();
  const onGroupRow=!!groupPick;
  const rowActions=textStyleRowActions(actions.map(a=>onGroupRow&&['options','duplicate','redefine','convertLists','copyToGroup'].includes(a.id)?{...a,disabled:true}:a));
  if(kind==='character')delete rowActions['id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText'];
  // the group commands are off on a style row, and the apply variants on a group row (the paragraph panel only has the variants)
  for(const [rid,row] of Object.entries(rowActions))if(rid.includes('.applyClearAll')||rid.includes('.applyThenNextStyle')||rid.includes('.applyClearCharacterStyles')){if(kind==='character'||onGroupRow)rowActions[rid]={...row,disabled:true};}
  if(!onGroupRow&&rowActions['id.ctx.styleParagraph.duplicateGroup'])rowActions['id.ctx.styleParagraph.duplicateGroup']={...rowActions['id.ctx.styleParagraph.duplicateGroup'],disabled:true};
  const rowLabels=kind==='character'?{'id.panel.paragraphStyles.newParagraphStyle':ctx.commands?.()['id.panel.characterStyles.newCharacterStyle']?.label??''}:{};
  for(const [rid,label] of Object.entries(rowLabels))if(rowActions[rid]&&label)rowActions[rid]={...rowActions[rid],label};
  useEffect(()=>()=>ctx.textStyleActions?.(kind,[]),[ctx.textStyleActions,kind]);
  const rows=textStyleRows(list,paths,closed,sorted);
  return <div className="styles-panel" data-text-styles-panel={kind}>
    <div className={small?'st-list small-rows':'st-list'} data-drop-top={dropAt===''?'':undefined} onDragOver={e=>overDrop(e,'')} onDragLeave={()=>setDropAt(null)} onDrop={e=>dropOn(e,[])}>{rows.map(r=>r.type==='group'
      ?<button key={'g:'+r.key} className="st-row st-group" style={{paddingLeft:10+r.depth*14}} data-text-style-group={r.key} aria-expanded={r.open} aria-pressed={!!pickedGroup&&groupKey(pickedGroup)===r.key}
        data-drop={dropAt===r.key?'':undefined} onDragOver={e=>overDrop(e,r.key)} onDrop={e=>dropOn(e,r.key.split('%3a'))}
        onClick={()=>{setGroupSel(true);setPickedGroup(r.key.split('%3a'));setClosed(c=>{const n=new Set(c);if(n.has(r.key))n.delete(r.key);else n.add(r.key);return n;});}}
        onContextMenu={e=>{setGroupSel(true);setPickedGroup(r.key.split('%3a'));rowMenu.open(e);}}
        onDoubleClick={()=>{if(locked)return;setGroupSel(true);setPickedGroup(r.key.split('%3a'));setSub('renameGroup');}} title={tr('{0} · 두 번 누르면 이름 바꾸기', { 0: r.name })}>{r.open?'▾ ':'▸ '}{r.name}</button>
      :<button key={r.style.id} className="st-row" style={{paddingLeft:10+r.depth*14}} disabled={locked}
        data-text-style={r.style.id} draggable={!locked&&!r.style.id.includes('/$ID/')} onDragStart={e=>{e.dataTransfer.setData(dragType,r.style.id);e.dataTransfer.effectAllowed='move';}} aria-pressed={picked!==null?picked.includes(r.style.id):id===r.style.id} title={tr('{0} · 두 번 누르면 옵션', { 0: r.style.name })}
        onContextMenu={e=>{setGroupSel(false);setPicked(null);setSelected(r.style.id);rowMenu.open(e);}}
        onClick={e=>{setGroupSel(false);setPicked(null);setSelected(r.style.id);if(selection)ctx.textFormatting.style(kind,r.style.id,e.altKey);}}
        onDoubleClick={()=>{setSelected(r.style.id);if(isRootStyle(r.style.id))return;getStyleInfo(kind,r.style.id).then(res=>setDialog({mode:'edit',info:res.style}),e=>setError(String(e)));}}>
        {r.style.name}{applied===r.style.id&&selection&&overridden?' +':''}
      </button>)}</div>
    <div className="p-foot style-actions">
      <button disabled={locked||!selection} onClick={()=>open('create')}>{tr('선택에서 새 스타일')}</button>
      <button disabled={locked||!id||rootStyle||several} onClick={()=>open('edit')}>{tr('옵션')}</button>
      <button disabled={locked||!selection||!id||rootStyle||several} onClick={redefine}>{tr('다시 정의')}</button>
      <button disabled={locked||(groupPick?false:!id||idProtected)} onClick={()=>groupPick?setSub('deleteGroup'):open('delete')}>{tr('삭제')}</button>
    </div>
    {error&&<p role="alert">{error}</p>}
    {sub==='duplicate'&&current&&<NameDialog title={tr('스타일 복제')} label={tr('이름:')} initial={`${current.name.replace(/^\[(.*)\]$/,'$1')} copy`} taken={n=>list.some(s=>s.id===styleId(current.group,n))} onOk={duplicate} close={()=>setSub(null)}/>}
    {sub==='newGroup'&&<NameDialog title={tr('새 스타일 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={n=>paths.some(p=>groupKey(p)===groupKey([...parent,n]))} onOk={newGroup} close={()=>setSub(null)}/>}
    {sub==='groupFromStyles'&&<NameDialog title={tr('스타일에서 새 그룹')} label={tr('이름:')} initial={tr('스타일 그룹')} taken={n=>paths.some(p=>groupKey(p)===groupKey([n]))} onOk={groupFromStyles} close={()=>setSub(null)}/>}
    {sub==='renameGroup'&&groupPick&&<NameDialog title={tr('스타일 그룹 이름 바꾸기')} label={tr('이름:')} initial={groupPick[groupPick.length-1]}
      taken={n=>n!==groupPick[groupPick.length-1]&&(paths.some(p=>groupKey(p)===groupKey([...groupPick.slice(0,-1),n]))||list.some(s=>s.id===styleId(groupPick.slice(0,-1),n)))} onOk={renameGroup} close={()=>setSub(null)}/>}
    {sub==='deleteGroup'&&groupPick&&<GroupDeleteDialog name={groupPick.join(' / ')} count={inGroup(groupPick).length} onOk={deleteGroup} close={()=>setSub(null)}/>}
    {sub==='copyTo'&&current&&<GroupPickDialog title={tr('그룹에 복사')} groups={paths.map(p=>({id:p.join(':'),path:p}))} close={()=>setSub(null)}
      onOk={g=>ctx.edit(tr('그룹에 복사'),[{op:'copyTextStyleToGroup',kind,style:current.id,group:g}])}/>}
    {(sub==='load'||sub==='loadAll')&&<LoadStylesDialog ctx={ctx} kinds={sub==='load'?kind:'text'} close={()=>setSub(null)}/>}
    <StyleRowMenu menu="styleParagraph" at={rowMenu.at} commands={ctx.commands} actions={rowActions} name={onGroupRow?groupPick!.join(' / '):(current?.name??'')} close={rowMenu.close}/>
    {dialog&&<StyleDialog key={dialog.mode+dialog.info.id} ctx={ctx} kind={kind} {...dialog} group={dialog.mode==='create'?parent:undefined} choose={setSelected} close={()=>setDialog(null)}/>}
  </div>;
}
