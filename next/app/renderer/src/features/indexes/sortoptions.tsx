// Index panel ▸ Sort Options (rest-c1): the language groups of the index, each on or off, in order, with its heading kind.
// The engine keeps them (setIndexSortOptions, IDML IndexingSortOption) and index generation follows them (indexes/sort_options.rs).
import { useState } from 'react';
import { tr } from '../../i18n';
import { Modal } from '../../ui/tables';

export type SortGroup = {name:string;include:boolean;priority:number;header_type:string};

const HEADERS: Record<string,string[]> = {
  kIndexGroup_Alphabet: ['BasicLatin','FinnishSwedish','DanishNorwegian','Spanish','Croatian','Czech','Estonian','Hungarian','Latvian','Lithuanian','Polish','Romanian','Slovak','Slovenian','Turkish'],
  kWRIndexGroup_CyrillicAlphabet: ['Russian','Belarusian','Bulgarian','Ukrainian'],
  kIndexGroup_Kana: ['HiraganaAll','HiraganaConsonantsOnly','KatakanaAll','KatakanaConsonantsOnly'],
  kIndexGroup_Chinese: ['ChinesePinyin','ChineseStrokeCount'],
  kIndexGroup_Korean: ['KoreanConsonant','KoreanConsonantPlusVowel'],
};

function groupLabel(name:string):string {
  switch(name) {
    case 'kIndexGroup_Symbol': return tr('기호');
    case 'kIndexGroup_Alphabet': return tr('로마자');
    case 'kIndexGroup_Numeric': return tr('숫자');
    case 'kWRIndexGroup_GreekAlphabet': return tr('그리스 문자');
    case 'kWRIndexGroup_CyrillicAlphabet': return tr('키릴 문자');
    case 'kIndexGroup_Kana': return tr('가나');
    case 'kIndexGroup_Chinese': return tr('한자');
    case 'kIndexGroup_Korean': return tr('한글');
    case 'kWRIndexGroup_ArabicAlphabet': return tr('아랍 문자');
    case 'kWRIndexGroup_HebrewAlphabet': return tr('히브리 문자');
    default: return name;
  }
}

function headerLabel(kind:string):string {
  switch(kind) {
    case 'BasicLatin': return tr('기본 라틴 (A~Z)');
    case 'FinnishSwedish': return tr('핀란드어·스웨덴어');
    case 'DanishNorwegian': return tr('덴마크어·노르웨이어');
    case 'Spanish': return tr('스페인어');
    case 'Croatian': return tr('크로아티아어');
    case 'Czech': return tr('체코어');
    case 'Estonian': return tr('에스토니아어');
    case 'Hungarian': return tr('헝가리어');
    case 'Latvian': return tr('라트비아어');
    case 'Lithuanian': return tr('리투아니아어');
    case 'Polish': return tr('폴란드어');
    case 'Romanian': return tr('루마니아어');
    case 'Slovak': return tr('슬로바키아어');
    case 'Slovenian': return tr('슬로베니아어');
    case 'Turkish': return tr('터키어');
    case 'Russian': return tr('러시아어');
    case 'Belarusian': return tr('벨라루스어');
    case 'Bulgarian': return tr('불가리아어');
    case 'Ukrainian': return tr('우크라이나어');
    case 'KoreanConsonant': return tr('한글 자음 (ㄱ, ㄴ, ㄷ…)');
    case 'KoreanConsonantPlusVowel': return tr('한글 자음과 모음 (가, 나, 다…)');
    case 'HiraganaAll': return tr('히라가나 전체');
    case 'HiraganaConsonantsOnly': return tr('히라가나 자음만');
    case 'KatakanaAll': return tr('가타카나 전체');
    case 'KatakanaConsonantsOnly': return tr('가타카나 자음만');
    case 'ChinesePinyin': return tr('병음');
    case 'ChineseStrokeCount': return tr('획수');
    default: return tr('그룹 이름');
  }
}

/** The dialog: `groups` as the document keeps them (priority order); OK sends every group with its place as its priority. */
export function SortOptionsDialog({groups,close,apply}:{groups:SortGroup[];close:()=>void;apply:(groups:SortGroup[])=>void}) {
  const [rows,setRows]=useState<SortGroup[]>(()=>[...groups].sort((a,b)=>a.priority-b.priority));
  const [picked,setPicked]=useState(0);
  const set=(i:number,change:Partial<SortGroup>)=>setRows(r=>r.map((g,k)=>k===i?{...g,...change}:g));
  const move=(by:number)=>{
    const to=picked+by;
    if(to<0||to>=rows.length)return;
    const next=[...rows];[next[picked],next[to]]=[next[to],next[picked]];
    setRows(next);setPicked(to);
  };
  return <Modal title={tr('정렬 옵션')} close={close} ok={rows.length>0} submit={()=>apply(rows.map((g,i)=>({...g,priority:i})))}>
    <p>{tr('색인에 머리글 묶음으로 넣을 언어와 순서를 고릅니다. 고르지 않은 언어의 주제는 기호 묶음에 들어갑니다.')}</p>
    <ul role="listbox" aria-label={tr('정렬 언어 묶음')} style={{listStyle:'none',margin:0,padding:0,display:'flex',flexDirection:'column',gap:4}}>
      {rows.map((g,i)=><li key={g.name} role="option" aria-selected={i===picked} data-sort-group={g.name} onClick={()=>setPicked(i)}
        style={{display:'flex',alignItems:'center',gap:8,padding:'2px 4px',background:i===picked?'var(--selection, rgba(80,140,255,0.18))':undefined}}>
        <input type="checkbox" checked={g.include} aria-label={tr('{0} 묶음 넣기',{0:groupLabel(g.name)})} style={{width:'auto',flex:'none',margin:0}} onChange={e=>set(i,{include:e.target.checked})}/>
        <span style={{flex:1}}>{groupLabel(g.name)}</span>
        <select value={g.header_type} aria-label={tr('{0} 머리글 종류',{0:groupLabel(g.name)})} onChange={e=>set(i,{header_type:e.target.value})} disabled={!(HEADERS[g.name]?.length)}>
          {(HEADERS[g.name]??[g.header_type]).map(h=><option key={h} value={h}>{headerLabel(h)}</option>)}
        </select>
      </li>)}
    </ul>
    <div style={{display:'flex',gap:8}}>
      <button type="button" disabled={picked===0} onClick={()=>move(-1)}>{tr('위로')}</button>
      <button type="button" disabled={picked>=rows.length-1} onClick={()=>move(1)}>{tr('아래로')}</button>
    </div>
  </Modal>;
}
