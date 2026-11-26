import {Fragment,useEffect,useState} from 'react';
import {tr} from '../../i18n';
import type {PanelCtx} from '../../ui/panels';
import type {Commands} from '../../ui/commands';
import {Dropdown} from '../../ui/menu';
import {CONTEXT_MENUS} from '../../ui/menudef';
import {declarePanelCommands,usePanelCommands} from '../../ui/panelbus';
import {fontFamilyProperties} from '../../ui/fontfamily';
import {textStory} from '../../engine';
import {fontGlyphs,type GlyphEntry,type GlyphPage,type GlyphFilter} from './query';
import {GlyphSetsBlock,SET_SHOW,useGlyphSets} from './sets';
import './panel.css';

export const GLYPH_PANELS = {
  glyphs:{get title(){return tr('글리프');},icon:'character',width:420,height:550},
};

// 최근 글리프 (InDesign 2025, rules/glyphs-c4.ko.md): one list for every document and font, newest first, a glyph inserted again moves to the front,
// at most 35 (the Recently Used row shows the first 27); each is drawn in its own font. Kept in this computer's user settings, not in a project.
const MAX_RECENT=35;
type Recent={family:string;style:string;id:number;cp:number|null};
type Cell=GlyphEntry&{family:string;style:string;em:number;rememberFont?:boolean;setCp?:number|null};
const cellKey=(r:{family:string;style:string;id:number})=>`${r.family}\t${r.style}\t${r.id}`;
function readRecent(v:unknown):Recent[] {
  if(!Array.isArray(v))return [];
  return v.filter((r):r is Recent=>!!r&&typeof r.family==='string'&&typeof r.style==='string'&&Number.isInteger(r.id)&&r.id>0&&(r.cp===null||Number.isInteger(r.cp))).slice(0,MAX_RECENT);
}
const FALLBACK_SHOW=[{id:'recent',name:'Recent Glyphs',group:0},{id:'alternates',name:'Alternates for Selection',group:1},{id:'entire',name:'Entire Font',group:1}];
// the Show item names: InDesign's English names from the engine, the panel's language here
function showName(id:string):string {
  const names:Record<string,string>={
    recent:tr('최근 글리프'),alternates:tr('선택 문자 대체'),entire:tr('전체 글꼴'),
    'range:latin':tr('기본 라틴과 라틴 1'),'range:latin-a':tr('확장 라틴 A'),'range:latin-b':tr('확장 라틴 B'),'range:punctuation':tr('문장 부호'),
    'range:open-close':tr('열고 닫는 문장 부호'),'range:dashes-quotes':tr('대시와 따옴표'),'range:scripts':tr('위 첨자와 아래 첨자'),'range:numbers':tr('숫자'),
    'range:currency':tr('통화'),'range:symbols':tr('기호'),'range:math':tr('수학 기호'),'range:greek':tr('그리스어'),'range:cyrillic':tr('키릴 문자'),
    'feature:ccmp':tr('글리프 합성·분해'),'feature:init':tr('어두 형태'),'feature:isol':tr('고립 형태'),'feature:medi':tr('어중 형태'),'feature:rlig':tr('필수 합자'),
    'feature:liga':tr('표준 합자'),'feature:fina':tr('어말 형태'),'feature:dnom':tr('분모'),'feature:frac':tr('분수'),'feature:lnum':tr('라이닝 숫자'),
    'feature:numr':tr('분자'),'feature:onum':tr('올드스타일 숫자'),'feature:pnum':tr('비례 숫자'),'feature:smcp':tr('작은 대문자 형태'),
    'feature:c2sc':tr('대문자에서 작은 대문자'),'feature:salt':tr('스타일 대체'),'feature:tnum':tr('고정폭 숫자'),'feature:unic':tr('유니케이스'),'feature:vert':tr('세로쓰기'),
    'feature:subs':tr('아래 첨자 형태'),'feature:sups':tr('위 첨자 형태'),
    'feature:case':tr('대소문자 구분 형태'),'feature:sinf':tr('과학 아래 첨자'),'feature:ordn':tr('서수 표기'),'feature:zero':tr('슬래시 있는 0'),'feature:hist':tr('옛 형태'),'feature:aalt':tr('모든 대체 글리프'),'feature:afrc':tr('대체 분수'),'feature:hlig':tr('옛 합자'),'feature:ornm':tr('장식 기호'),'feature:falt':tr('줄 끝 글리프 대체'),'feature:swsh':tr('스워시'),'feature:fwid':tr('전각 너비'),'feature:hwid':tr('반각 너비'),'feature:pwid':tr('비례 너비'),'feature:ruby':tr('루비 표기 형태'),'feature:vrt2':tr('세로 대체와 회전'),'feature:dlig':tr('선택 합자'),
  };
  const set=/^feature:ss(\d\d)$/.exec(id);
  return set?tr('스타일 세트 {0}',{0:Number(set[1])}):names[id]??id;
}
const unicode=(cp:number)=>`U+${cp.toString(16).toUpperCase().padStart(4,'0')}`;
function Preview({row,em}:{row:GlyphEntry;em:number}) {
  const [x0,y0,x1,y1]=row.bounds??[0,0,Math.max(row.advance,em/2),em];
  const pad=em/10,w=Math.max(x1-x0,em/4),h=Math.max(y1-y0,em/4);
  return <svg viewBox={`${x0-pad} ${-y1-pad} ${w+pad*2} ${h+pad*2}`} aria-hidden="true"><path d={row.path} transform="scale(1,-1)"/></svg>;
}
// The panel menu and the right-click menus of a glyph cell (menuaudit-c): what each item does, by command id.
const GM = 'id.panel.glyphs.', GC = 'id.ctx.glyphs.', GR = 'id.ctx.glyphsRecent.';
declarePanelCommands([GM+'sortGlyphs.byCidGid', GM+'sortGlyphs.byUnicode', GC+'loadGlyphInFind', GC+'loadGlyphInChange', GR+'deleteGlyphFromRecentlyUsed', GR+'clearAllRecentlyUsed',
  // glyph sets (build-c)
  GM+'newGlyphSet', 'glyphs.addToSet', 'glyphs.editSet', 'glyphs.viewSet', 'glyphs.deleteSet', 'glyphs.importSets', 'glyphs.exportSets', GC+'deleteGlyphFromSet']);

export function GlyphsPanel({ctx,commands}:{ctx:PanelCtx;commands?:Commands}) {
  const control=ctx.textFormatting, selection=control.selection;
  const [fonts,setFonts]=useState<Record<string,string[]>>({});
  const [family,setFamily]=useState(''),[style,setStyle]=useState('');
  const [offset,setOffset]=useState(0),[search,setSearch]=useState(''),[show,setShow]=useState('entire');
  // the Show item (InDesign's list: Recent Glyphs, Alternates for Selection, Entire Font, a range, an OpenType feature); the engine lists the items of the face
  const filter:GlyphFilter=show==='alternates'?'alternates':'all',view=/^(range|feature):/.test(show)?show:'';
  // 최근 글리프: the user settings' list, and the entries (outlines) of it that the Recent Glyphs view fetched from the engine
  const [recent,setRecent]=useState<Recent[]>([]);
  const [recentCells,setRecentCells]=useState<Record<string,Cell>>({});
  useEffect(()=>{let on=true;window.folio?.settings?.().then(s=>{if(on)setRecent(readRecent(s.recentGlyphs));}).catch(()=>{});return()=>{on=false;};},[]);
  // the glyph sets (build-c): a set shown in the Show list is listed like the recent glyphs, each glyph in its own face
  const sets=useGlyphSets();
  const shownSet=show.startsWith(SET_SHOW)?sets.sets.find(s=>SET_SHOW+s.name===show)??null:null;
  const listed:{family:string;style:string;id:number}[]|null=show==='recent'?recent:shownSet?shownSet.glyphs:null;
  useEffect(()=>{if(show.startsWith(SET_SHOW)&&!shownSet)setShow('entire');},[show,shownSet]);
  const recentList=(listed??[]).map(cellKey).join('\n');
  useEffect(()=>{
    if(!listed?.length)return;
    let on=true;
    const faces=[...new Set(listed.map(r=>`${r.family}\t${r.style}`))];
    Promise.all(faces.map(f=>{
      const [family,style]=f.split('\t');
      return fontGlyphs(family,style,0,'','all','','unicode',`ids:${listed.filter(r=>r.family===family&&r.style===style).map(r=>r.id).join(',')}`)
        .then(p=>p.entries.map(e=>[cellKey({family,style,id:e.id}),{...e,family,style,em:p.unitsPerEm}] as const)).catch(()=>[]);
    })).then(rs=>{if(on)setRecentCells(Object.fromEntries(rs.flat()));});
    return ()=>{on=false;};
  },[show,recentList]);
  const [page,setPage]=useState<GlyphPage|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(false);
  const [chosen,setChosen]=useState<string|null>(null),[codepoint,setCodepoint]=useState<number|null>(null);
  // 선택 문자 대체: the selected text the alternates are listed for
  const [selected,setSelected]=useState('');
  // the panel menu's By Unicode (InDesign's default: the panel opens with it checked) and By CID / GID: the order of the whole list
  const [order,setOrder]=useState<'gid'|'unicode'>('unicode');
  useEffect(()=>{if(selection)setFonts(selection.fonts);},[selection?.fonts]);
  useEffect(()=>{
    if(selection?.format.appliedFont && selection.format.fontStyle){
      setFamily(String(selection.format.appliedFont));setStyle(String(selection.format.fontStyle));setOffset(0);
    }
  },[selection?.format.appliedFont,selection?.format.fontStyle]);
  useEffect(()=>{
    let active=true;
    if(filter!=='alternates'||!selection){setSelected('');return;}
    // a bare insertion point leaves the list as it was (InDesign 2025 panel, measured with a real caret: rules/glyphs-c3.ko.md)
    if(selection.start===selection.end)return;
    textStory(selection.story).then(s=>{if(active)setSelected(Array.from(s.text).slice(selection.start,selection.end).join(''));}).catch(()=>{if(active)setSelected('');});
    return ()=>{active=false;};
  },[filter,selection?.story,selection?.start,selection?.end]);
  useEffect(()=>{
    let active=true;
    setPage(null);setChosen(null);setCodepoint(null);setError(false);
    if(!ctx.doc||!family||!style||(filter==='alternates'&&!selected)){setBusy(false);return;}
    setBusy(true);
    const timer=setTimeout(()=>{
      fontGlyphs(family,style,offset,search,filter,selected,order,view).then(p=>{if(active)setPage(p);})
        .catch(()=>{if(active)setError(true);}).finally(()=>{if(active)setBusy(false);});
    },100);
    return ()=>{active=false;clearTimeout(timer);};
  },[ctx.doc?.path,family,style,offset,search,filter,selected,order,view]);
  // a face without the chosen Show item (another face's feature or range) goes back to Entire Font
  useEffect(()=>{if(page&&!show.startsWith(SET_SHOW)&&!page.showItems.some(i=>i.id===show))setShow('entire');},[page?.family,page?.style,page?.showItems]);
  const list:Cell[]=show==='recent'?recent.flatMap(r=>recentCells[cellKey(r)]??[])
    :shownSet?shownSet.glyphs.flatMap(g=>{const c=recentCells[cellKey(g)];return c?[{...c,rememberFont:g.rememberFont,setCp:g.cp}]:[];})
    :page?page.entries.map(e=>({...e,family:page.family,style:page.style,em:page.unitsPerEm})):[];
  const row=list.find(r=>cellKey(r)===chosen);
  // a glyph with no Unicode value (or only control codes) goes in by its glyph id: the engine stores it as U+FFFD + CustomGlyph
  const byId=!!row&&row.id>0&&!row.insertable.length;
  const canInsert=!!selection&&ctx.editable&&!!row&&(byId||(codepoint!==null&&row.insertable.includes(codepoint)));
  const pick=(r:Cell)=>{setChosen(cellKey(r));setCodepoint(r.setCp!=null&&r.unicode.includes(r.setCp)?r.setCp:r.insertable[0]??r.unicode[0]??null);};
  const addToSet=()=>{if(row&&sets.current)void sets.run({op:'add',name:sets.target,glyph:{family:row.family,style:row.style,id:row.id,cp:codepoint??row.unicode[0]??null}});};
  const viewSet=()=>{if(sets.current){setShow(SET_SHOW+sets.target);setOffset(0);}};
  const [menu,setMenu]=useState<{x:number;y:number}|null>(null);
  const saveRecent=(next:Recent[])=>{setRecent(next);void window.folio?.saveSettings?.({recentGlyphs:next}).catch(()=>{});};
  usePanelCommands('glyphs',{
    [GM+'sortGlyphs.byCidGid']:filter==='alternates'?undefined:{run:()=>{setOrder('gid');setOffset(0);},checked:order==='gid'},
    [GM+'sortGlyphs.byUnicode']:filter==='alternates'?undefined:{run:()=>{setOrder('unicode');setOffset(0);},checked:order==='unicode'},
    [GC+'loadGlyphInFind']:row&&ctx.loadGlyph?()=>ctx.loadGlyph?.('find',{family:row.family,style:row.style,glyphId:row.id}):undefined,
    [GC+'loadGlyphInChange']:row&&ctx.loadGlyph?()=>ctx.loadGlyph?.('change',{family:row.family,style:row.style,glyphId:row.id}):undefined,
    [GR+'deleteGlyphFromRecentlyUsed']:show==='recent'&&row?()=>{saveRecent(recent.filter(x=>cellKey(x)!==cellKey(row)));setChosen(null);}:undefined,
    [GR+'clearAllRecentlyUsed']:show==='recent'&&recent.length?()=>{saveRecent([]);setChosen(null);}:undefined,
    [GM+'newGlyphSet']:()=>sets.setForm('new'),
    ['glyphs.addToSet']:row&&sets.current?addToSet:undefined,
    ['glyphs.editSet']:sets.current?()=>sets.setForm('edit'):undefined,
    ['glyphs.viewSet']:sets.current?viewSet:undefined,
    ['glyphs.deleteSet']:sets.current?()=>void sets.run({op:'delete',name:sets.target}):undefined,
    ['glyphs.importSets']:ctx.doc?.folio?()=>sets.setForm('import'):undefined,
    ['glyphs.exportSets']:ctx.doc?.folio&&sets.sets.length?()=>sets.setForm('export'):undefined,
    [GC+'deleteGlyphFromSet']:shownSet&&row?()=>{const i=shownSet.glyphs.findIndex(g=>cellKey(g)===cellKey(row));if(i>=0)void sets.run({op:'remove',name:shownSet.name,index:i});setChosen(null);}:undefined,
  });
  const insert=(r=row,cp=codepoint)=>{
    if(!selection||!ctx.editable||!r)return;
    const remember=(used:number|null)=>{
      const next=[{family:r.family,style:r.style,id:r.id,cp:used},...recent.filter(x=>cellKey(x)!==cellKey(r))].slice(0,MAX_RECENT);
      setRecent(next);void window.folio?.saveSettings?.({recentGlyphs:next}).catch(()=>{});
    };
    // a set glyph without Remember Font goes in by its Unicode value in the font of the text
    if(r.rememberFont===false&&cp!==null){control.insertGlyph(String.fromCodePoint(cp),{});remember(cp);return;}
    if(!r.insertable.length){if(r.id>0){control.insertGlyphById(r.family,r.style,r.id);remember(null);}return;}
    if(cp===null||!r.insertable.includes(cp))return;
    control.insertGlyph(String.fromCodePoint(cp),{appliedFont:r.family,fontStyle:r.style});remember(cp);
  };
  return <div className="glyphs-panel" onKeyDown={e=>e.stopPropagation()}>
    <label>{tr('글꼴')}<select aria-label={tr('글리프 글꼴')} value={family} onChange={e=>{
      const props=fontFamilyProperties(fonts,e.target.value,style);setFamily(String(props.appliedFont));setStyle(String(props.fontStyle));setOffset(0);
    }}><option value="">{tr('글꼴 선택')}</option>{Object.keys(fonts).map(f=><option key={f}>{f}</option>)}</select></label>
    <label>{tr('스타일')}<select aria-label={tr('글리프 글꼴 스타일')} value={style} onChange={e=>{setStyle(e.target.value);setOffset(0);}}>
      {(fonts[family]??[style]).map(s=><option key={s}>{s}</option>)}
    </select></label>
    <label>{tr('표시')}<select aria-label={tr('글리프 필터')} value={show} onChange={e=>{setShow(e.target.value);setOffset(0);}}>
      {(page?.showItems??FALLBACK_SHOW).map((it,i,all)=><Fragment key={it.id}>
        {i>0&&all[i-1].group!==it.group&&<option disabled>{'──────────'}</option>}<option value={it.id}>{showName(it.id)}</option>
        {it.id==='recent'&&sets.sets.map(s=><option key={s.name} value={SET_SHOW+s.name}>{s.name}</option>)}</Fragment>)}
    </select></label>
    <label>{tr('목록 순서')}<select aria-label={tr('글리프 정렬')} value={order} disabled={filter==='alternates'} onChange={e=>{setOrder(e.target.value as 'gid'|'unicode');setOffset(0);}}>
      <option value="unicode">{tr('Unicode순')}</option><option value="gid">{tr('CID / GID순')}</option>
    </select></label>
    <label>{tr('검색')}<input aria-label={tr('글리프 검색')} placeholder={tr('문자, U+코드, 글리프 이름 또는 ID')} value={search} onChange={e=>{setSearch(e.target.value);setOffset(0);}}/></label>
    {!family&&<p>{tr('문자 도구로 삽입 위치를 선택하세요.')}</p>}
    {filter==='alternates'&&!selected&&<p>{tr('글자를 선택하면 그 글자의 대체 글리프를 보여 줍니다.')}</p>}
    {error&&<p role="alert">{tr('글꼴의 글리프 표를 읽을 수 없습니다.')}</p>}
    {busy&&<p role="status">{tr('글리프 읽는 중…')}</p>}
    {(page||listed)&&<>
      <div className="glyphs-grid" aria-label={tr('글리프 목록')}>
        {list.map(r=><button key={cellKey(r)} data-glyph-id={r.id} data-insertable={r.insertable.length>0} aria-pressed={chosen===cellKey(r)}
          title={`${r.name??tr('이름 없음')} · ${r.unicode.map(unicode).join(', ')||tr('Unicode 없음')} · GID ${r.id}${r.features.length?` · ${r.features.join(' ')}`:''}`}
          onClick={()=>pick(r)} onDoubleClick={()=>insert(r,r.insertable[0]??null)}
          onContextMenu={e=>{e.preventDefault();e.stopPropagation();pick(r);setMenu({x:e.clientX,y:e.clientY});}}>
          <Preview row={r} em={r.em}/><small>{r.id}</small>
        </button>)}
      </div>
      {!list.length&&<p>{tr('해당하는 글리프가 없습니다.')}</p>}
      {page&&!listed&&<div className="glyphs-pages"><button aria-label={tr('이전 글리프 페이지')} disabled={!offset} onClick={()=>setOffset(Math.max(0,offset-page.limit))}>{tr('이전')}</button>
        <span>{tr('{0}개 중 {1}–{2}',{0:page.total,1:page.entries.length?offset+1:0,2:Math.min(offset+page.limit,page.total)})}</span>
        <button aria-label={tr('다음 글리프 페이지')} disabled={offset+page.limit>=page.total} onClick={()=>setOffset(offset+page.limit)}>{tr('다음')}</button></div>}
    </>}
    {row&&<div className="glyphs-detail">
      <div>{row.name??tr('이름 없음')} · GID {row.id}{row.features.length>0&&<> · {row.features.join(' ')}</>}</div>
      <select aria-label={tr('삽입할 Unicode')} value={codepoint??''} disabled={!row.unicode.length} onChange={e=>setCodepoint(Number(e.target.value))}>
        {!row.unicode.length&&<option value="">{tr('Unicode 없음')}</option>}
        {row.unicode.map(cp=><option key={cp} value={cp}>{unicode(cp)}</option>)}
      </select>
      {byId&&<p>{tr('Unicode 없는 글리프는 글리프 번호로 삽입합니다.')}</p>}
      {!row.insertable.length&&!byId&&<p>{tr('이 글리프는 문자로 삽입할 수 없습니다.')}</p>}
    </div>}
    <button aria-label={tr('글리프 삽입')} disabled={!canInsert} onClick={()=>insert()}>{tr('삽입')}</button>
    <GlyphSetsBlock state={sets} canAdd={!!row} onAdd={addToSet} onView={viewSet}/>
    {!selection&&family&&<p>{tr('문자 도구로 삽입 위치를 선택하세요.')}</p>}
    {menu&&commands&&<Dropdown items={CONTEXT_MENUS[show==='recent'?'glyphsRecent':'glyphs']??[]} commands={commands} x={menu.x} y={menu.y} onClose={()=>setMenu(null)}/>}
  </div>;
}
