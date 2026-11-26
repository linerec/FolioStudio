import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import { swatchUsage, type EditCommand, type EditResult, type InkInfo } from '../../engine';
import type { DocumentInfo } from '../../inspect';
import type { Command } from '../../ui/commands';
import { SwatchDialog, DeleteSwatchDialog, editable, type Swatch } from '../../ui/swatchedit';
import { type AseColor, type AseGroup, readAse, writeAse } from './ase';

type Dialog = {kind: 'new'|'edit'|'delete'; id: string|null};
export type ColorGroups = NonNullable<DocumentInfo['colorGroups']>;
type Edit = (label:string, commands:EditCommand[])=>Promise<EditResult|null>;
export function useColorMenu({swatches, groups, mixedGroups, inks, canEdit, edit, selectItems, flash}: {swatches: Swatch[]; groups: ColorGroups; mixedGroups: Record<string, string[]>; inks: InkInfo[]; canEdit: boolean; edit: Edit; selectItems: (ids: string[])=>Promise<number>; flash?: (message: string)=>void}) {
  // the swatches picked in the panel (click order: Merge Swatches keeps the first) or one colour group
  const [picked, setPicked] = useState<string[]>([]);
  const [group, setGroup] = useState<string|null>(null);
  // a mixed ink group picked in the panel (its header row): Swatch Options renames it, Delete Swatch deletes it with its swatches
  const [mixedGroup, setMixedGroup] = useState<string|null>(null);
  const [dialog, setDialog] = useState<Dialog|null>(null);
  const [view, setView] = useState('smallList');
  const [filter, setFilter] = useState('all');
  const select = (ids: string[]) => { setPicked(ids); setGroup(null); setMixedGroup(null); };
  const selectGroup = (id: string) => { setPicked([]); setGroup(id); setMixedGroup(null); };
  const selectMixedGroup = (id: string) => { setPicked([]); setGroup(null); setMixedGroup(id); };
  const toggle = (id: string) => { setPicked(p=>p.includes(id) ? p.filter(x=>x!==id) : [...p, id]); setGroup(null); setMixedGroup(null); };
  /** Shift+click: from the swatch picked first to this one in the order shown. */
  const range = (id: string, shown: string[]) => {
    const from = shown.indexOf(picked[0] ?? id), to = shown.indexOf(id);
    if (from < 0 || to < 0) { select([id]); return; }
    const [a, b] = from < to ? [from, to] : [to, from];
    const rows = shown.slice(a, b + 1);
    select(from <= to ? rows : rows.reverse());
  };
  const selected = swatches.find(s=>s.id===picked[0]);
  const pickedSwatches = picked.map(id=>swatches.find(s=>s.id===id)).filter((s): s is Swatch => !!s);
  const loose = pickedSwatches.filter(editable);
  const named = groups.find(g=>g.id===group&&!g.root)??null;
  const mixedSel = mixedGroup&&mixedGroups[mixedGroup] ? mixedGroup : null;
  // New Gradient Swatch / a gradient's Swatch Options: the shared dialogs `newGradient` and `gradientOptions` (next/shared/dialogs)
  // New Color Group / Color Group Options are the shared dialogs `newColorGroup` (the picked swatches) and `colorGroupOptions`; the panel
  // follows what they did (the picks cleared, the renamed group picked)
  const openDialog = (request: Record<string, unknown>)=>void window.folio.request({cmd:'openDialog',...request}).then(({header})=>{if(!header.ok)flash?.(String(header.error));});
  useEffect(()=>window.folio.onEvent(event=>{
    const done=event.event==='dialog-state'?(event.detail as {applied?:{dialog:string;values:Record<string,unknown>}}).applied:undefined;
    if(done?.dialog==='newColorGroup')setPicked([]);
    if(done?.dialog==='colorGroupOptions')setGroup(`ColorGroup/${String(done.values.name).replace(/%/g,'%25')}`);
    // a mixed ink group renamed stays picked under its new id; one deleted is no longer picked
    if(done?.dialog==='mixedInkGroupOptions')selectMixedGroup(`MixedInkGroup/${String(done.values.name).replace(/%/g,'%25')}`);
    if(done?.dialog==='mixedInkGroupDelete')setMixedGroup(null);
  }),[]);
  const newGroup = ()=>openDialog({dialog:'newColorGroup',ids:loose.map(s=>s.id),values:{name:tr('새 색상 그룹')}});
  const openGradient = (id: string|null=null)=>void window.folio.request(id?{cmd:'openDialog',dialog:'gradientOptions',ids:[id]}:{cmd:'openDialog',dialog:'newGradient',values:{name:tr('새 그레이디언트 견본')}})
    .then(({header})=>{if(!header.ok)flash?.(String(header.error));});
  const open = (kind: Dialog['kind'])=>setDialog({kind,id:selected?.id??null});
  const handlers: Record<string,Partial<Command>> = {};
  const command=(id:string, run:(()=>void)|undefined, checked?:boolean)=>{handlers[`id.panel.swatches.${id}`]={run,checked};};
  command('newColorSwatch',canEdit?()=>open('new'):undefined);
  command('newGradientSwatch',canEdit?()=>openGradient():undefined);
  // the Ink Manager's inks (the plates of the document) go into a mixed ink; one has to be a spot ink
  // New Mixed Ink Swatch is the shared dialog `newMixedInk` (the engine lists the inks; the name continues the word given)
  command('newMixedInkSwatch',canEdit&&inks.some(i=>i.kind==='spot')?()=>openDialog({dialog:'newMixedInk',values:{namePrefix:tr('혼합 잉크 견본')}}):undefined);
  command('newTintSwatch',canEdit&&selected&&['process','spot'].includes(selected.model)?()=>{void window.folio.request({cmd:'openDialog',dialog:'newTint',values:{base:selected.id}});}:undefined);
  const openMixedGroup=(id:string)=>openDialog({dialog:'mixedInkGroupOptions',ids:[id]});
  command('swatchOptions',canEdit&&mixedSel?()=>openMixedGroup(mixedSel):canEdit&&selected&&editable(selected)&&selected.model!=='tint'?()=>selected.model==='gradient'?openGradient(selected.id):open('edit'):undefined);
  const deleteMixedGroup=canEdit&&mixedSel?()=>openDialog({dialog:'mixedInkGroupDelete',ids:[mixedSel]}):undefined;
  command('deleteSwatch',deleteMixedGroup??(canEdit&&selected&&editable(selected)?()=>open('delete'):undefined));
  command('duplicateSwatch',canEdit&&selected&&['process','spot'].includes(selected.model)?()=>void edit(tr('색상 견본 복제'),[{op:'duplicateSwatch',id:selected.id}]):undefined);
  for(const mode of ['smallList','largeList','smallThumbnail','largeThumbnail']) command(mode,()=>setView(mode),view===mode);
  // Sort: all swatches, or the ones picked (they keep the places they hold)
  for(const [id,by,only] of [['allSwatchesByName','name',false],['allSwatchesByColorValues','value',false],['selectedSwatchesByName','name',true],['selectedSwatchesByColorValues','value',true]] as const) {
    const label = by==='name' ? tr('이름순 정렬') : tr('색상 값순 정렬');
    command(`sort.${id}`,canEdit&&(!only||loose.length>1)?()=>void edit(label,[{op:'sortSwatches',by,...(only?{only:loose.map(s=>s.id)}:{})}]):undefined);
  }
  command('selectAllUnused',()=>void swatchUsage().then(u=>{
    const free = swatches.filter(s=>editable(s)&&u.unused.includes(s.id)).map(s=>s.id);
    select(free);
    if (!free.length) flash?.(tr('사용하지 않는 견본이 없습니다.'));
  }).catch(e=>flash?.(String(e?.message??e))));
  command('addUnnamedColors',canEdit?()=>void edit(tr('이름 없는 색상 추가'),[{op:'addUnnamedColors'}]):undefined);
  command('findThisColor',selected&&picked.length===1&&selected.model!=='none'?()=>void swatchUsage().then(async u=>{
    const n = await selectItems(u.used[selected.id]?.items??[]);
    if (!n) flash?.(tr('이 색상을 쓰는 개체가 없습니다.'));
  }).catch(e=>flash?.(String(e?.message??e))):undefined);
  command('mergeSwatches',canEdit&&loose.length>1&&loose.length===pickedSwatches.length?()=>void edit(tr('색상 견본 병합'),[{op:'mergeSwatches',ids:loose.map(s=>s.id),keep:loose[0].id}]).then(()=>select([loose[0].id])):undefined);
  command('newColorGroup',canEdit&&loose.length>0?newGroup:undefined);
  command('colorGroupOptions',canEdit&&named?()=>openDialog({dialog:'colorGroupOptions',ids:[named.id]}):undefined);
  command('ungroupColorGroup',canEdit&&named?()=>void edit(tr('색상 그룹 해제'),[{op:'ungroupColorGroup',id:named.id}]).then(()=>setGroup(null)):undefined);
  // Load Swatches / Save Swatches: Adobe Swatch Exchange files; the colours' values go into the document, no path is kept
  const fixed = (s: Swatch) => !editable(s);
  const aseOf = (s: Swatch): AseColor | null => (s.model === 'process' || s.model === 'spot') && s.base === null && ['CMYK', 'RGB', 'LAB'].includes(s.space.toUpperCase())
    ? { name: s.name, space: s.space.toUpperCase() as AseColor['space'], values: s.values, spot: s.model === 'spot' } : null;
  const saveRows = (picked.length ? pickedSwatches : swatches).filter((s) => !fixed(s)).map((s) => ({ s, c: aseOf(s) })).filter((r): r is { s: Swatch; c: AseColor } => !!r.c);
  command('saveSwatches',saveRows.length?()=>void (async()=>{
    const ase = new Map<string | null, AseGroup>();
    for (const { s, c } of saveRows) {
      const g = groups.find((x) => !x.root && x.swatches.includes(s.id));
      const key = g ? g.name : null;
      if (!ase.has(key)) ase.set(key, { name: key, colors: [] });
      ase.get(key)!.colors.push(c);
    }
    const saved = await window.folio.swatchFileSave(`${tr('색상 견본')}.ase`, writeAse([...ase.values()]));
    if (saved) flash?.(tr('색상 견본 {0}개를 저장했습니다: {1}', { 0: saveRows.length, 1: saved }));
  })().catch(e=>flash?.(String(e?.message??e))):undefined);
  command('loadSwatches',canEdit?()=>void (async()=>{
    const bytes = await window.folio.swatchFileOpen();
    if (!bytes) return;
    let file: AseGroup[];
    try { file = readAse(bytes); } catch { flash?.(tr('색상 견본 파일을 읽을 수 없습니다.')); return; }
    const taken = new Set(swatches.map((x) => x.name));
    const near = (a: number[], b: number[]) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < 0.01);
    const fresh = (name: string) => { let n = name, k = 1; while (taken.has(n)) n = `${name} ${++k}`; taken.add(n); return n; };
    const commands: EditCommand[] = [];
    const grouped: { name: string; ids: string[] }[] = [];
    for (const g of file) {
      const ids: string[] = [];
      for (const c of g.colors) {
        // the same colour under the same name is already there
        const have = swatches.find((x) => x.name === c.name && x.model !== 'gradient' && x.base === null && x.space.toUpperCase() === c.space && near(x.values, c.values));
        if (have) continue;
        const name = fresh(c.name);
        commands.push({ op: 'newSwatch', name, spot: c.spot, space: c.space.toLowerCase(), value: c.values });
        ids.push(`Color/${name.replace(/%/g, '%25')}`);
      }
      if (g.name !== null && ids.length) {
        let gname = g.name;
        for (let k = 2; groups.some((x) => x.name === gname) || grouped.some((x) => x.name === gname); k++) gname = `${g.name} ${k}`;
        grouped.push({ name: gname, ids });
      }
    }
    if (!commands.length) { flash?.(tr('불러올 새 색상 견본이 없습니다.')); return; }
    for (const g of grouped) commands.push({ op: 'newColorGroup', name: g.name, swatches: g.ids });
    await edit(tr('색상 견본 불러오기'), commands);
  })().catch(e=>flash?.(String(e?.message??e))):undefined);
  const from=swatches.find(s=>s.id===dialog?.id)??null;
  // New Mixed Ink Group is the shared dialog `newMixedInkGroup` (the engine lists the inks and the swatches the group would make)
  command('newMixedInkGroup',canEdit&&inks.some(i=>i.kind==='spot')?()=>openDialog({dialog:'newMixedInkGroup',values:{namePrefix:tr('혼합 잉크 그룹')}}):undefined);
  const close=()=>setDialog(null);
  const node=dialog?(dialog.kind==='delete'?(from?<DeleteSwatchDialog swatch={from} swatches={swatches} edit={edit} close={close}/>:null)
    :<SwatchDialog mode={dialog.kind} from={from} swatches={swatches} edit={edit} close={close}/>):null;
  return {picked,select,toggle,range,group,selectGroup,mixedGroup:mixedSel,selectMixedGroup,openMixedGroup:canEdit?openMixedGroup:undefined,deleteMixedGroup,groups,view,filter,setFilter,openGradient,newGroup:canEdit&&loose.length>0?newGroup:undefined,handlers,node};
}
export type ColorMenu=ReturnType<typeof useColorMenu>;
