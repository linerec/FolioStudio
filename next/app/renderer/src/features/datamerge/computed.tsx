import {useEffect,useMemo,useRef,useState} from 'react';
import {tr} from '../../i18n';
import type {PanelCtx} from '../../ui/panels';
import type {DataMergeState} from './state';
import {evaluateComputedFields,readFunctions,type ComputedField,type Evaluation,type FunctionInfo,type RecordError} from './query';
import {baseName,bracketInfo,errorWithPlace,fieldRef,functionHelp,kindOfName,newName} from './expr';

type Kind='text'|'image'|'qrcode';

/** The report line of a record whose expression failed: its number, the field and what went wrong. */
export function recordErrorText(e:RecordError):string {
  return tr('레코드 {0} · {1}: {2}',{0:e.record,1:e.field,2:errorWithPlace(e,true)});
}

/** The "계산 열" group of the Data Merge panel: fields an expression makes from the columns of the data source. The values shown are the
 *  engine's (`dataMergeEvaluate`); Apply is one engine command (`setDataMergeComputedFields`, one undo step). */
export function ComputedFields({state,ctx,canInsert,insertField}:{state:DataMergeState;ctx:PanelCtx;canInsert:(kind:Kind)=>boolean;insertField:(name:string,kind:Kind)=>void}) {
  const catalogue=state.catalogue;
  // compared as pairs: the engine's objects have their keys in another order than the boxes'
  const key=(list:ComputedField[])=>JSON.stringify(list.map(f=>[f.name,f.expr]));
  const appliedKey=key(catalogue?.computed?.fields??[]);
  const applied=useMemo<ComputedField[]>(()=>(JSON.parse(appliedKey) as [string,string][]).map(([name,expr])=>({name,expr})),[appliedKey]);
  const [drafts,setDrafts]=useState<ComputedField[]>(applied);
  // the list the document has is what the boxes show again whenever it changes (apply, undo, redo, another document)
  useEffect(()=>{setDrafts(applied);},[applied]);
  const draftKey=key(drafts),dirty=draftKey!==appliedKey;
  const records=catalogue?.source.records.length??0;
  const record=Math.min(Math.max(state.previewRecord,1),Math.max(records,1));
  const [evaluation,setEvaluation]=useState<Evaluation|null>(null),[evaluationError,setEvaluationError]=useState('');
  const [functions,setFunctions]=useState<FunctionInfo[]>([]);
  const [applying,setApplying]=useState(false),[focused,setFocused]=useState(0);
  const [caret,setCaret]=useState<{row:number;at:number}>({row:0,at:0});
  const boxes=useRef<(HTMLTextAreaElement|null)[]>([]);
  useEffect(()=>{
    if(!catalogue||!drafts.length){setEvaluation(null);setEvaluationError('');return;}
    let current=true;
    const timer=setTimeout(()=>{
      void evaluateComputedFields(drafts,record,state.link?undefined:state.path,state.importOptions)
        .then(r=>{if(current){setEvaluation(r);setEvaluationError('');}})
        .catch(e=>{if(current)setEvaluationError(String(e));});
    },200);
    return ()=>{current=false;clearTimeout(timer);};
  },[draftKey,record,catalogue?.path,catalogue?.sha256,state.link?.state]);
  useEffect(()=>{if(catalogue&&!functions.length)void readFunctions().then(setFunctions).catch(()=>{});},[!!catalogue]);
  if(!catalogue)return null;
  const rawColumns=new Set(catalogue.computed?.added.map(a=>a.column)??[]);
  const rawFields=catalogue.source.fields.filter((_,i)=>!rawColumns.has(i));
  const evaluated=(i:number)=>{
    const f=evaluation?.fields[i];
    return f&&evaluation?.fields.length===drafts.length&&f.name===drafts[i].name&&f.expr===drafts[i].expr?f:null;
  };
  const complete=drafts.every(d=>d.name.trim()&&d.expr.trim());
  const failing=!!evaluation&&evaluation.fields.length===drafts.length&&evaluation.fields.some(f=>f.definitionError);
  const change=(i:number,patch:Partial<ComputedField>)=>setDrafts(d=>d.map((x,k)=>k===i?{...x,...patch}:x));
  const apply=async()=>{
    setApplying(true);
    try{
      const result=await ctx.edit(tr('계산 열 적용'),[{op:'setDataMergeComputedFields',fields:drafts}]);
      if(result)state.reload();
    } finally{setApplying(false);}
  };
  const put=(row:number,piece:string,back=0)=>{
    const box=boxes.current[row],text=drafts[row]?.expr??'';
    const from=box?box.selectionStart:text.length,to=box?box.selectionEnd:text.length;
    change(row,{expr:text.slice(0,from)+piece+text.slice(to)});
    const at=from+piece.length-back;
    requestAnimationFrame(()=>{box?.focus();box?.setSelectionRange(at,at);setCaret({row,at});});
  };
  const above=(row:number)=>[...rawFields,...drafts.slice(0,row).filter(d=>d.name.trim()).map(d=>({name:baseName(d.name),kind:kindOfName(d.name)}))];
  const errors:RecordError[]=evaluation?.recordErrors??catalogue.computed?.recordErrors??[];
  const errorCount=evaluation?.errorCount??catalogue.computed?.errorCount??0;
  const stepper=(by:number)=>state.setPreviewRecord(r=>Math.min(Math.max(r+by,1),Math.max(records,1)));
  return <fieldset className="datamerge-computed" data-computed-group="">
    <legend>{tr('계산 열')}</legend>
    <p>{tr('열을 합치거나 가공한 값을 필드로 씁니다. 식은 엑셀과 비슷합니다.')} <code>{tr('예: {성} & " " & {이름}')}</code></p>
    <div className="datamerge-record" role="group" aria-label={tr('미리 보기 레코드')}>
      <button type="button" aria-label={tr('이전 레코드')} disabled={record<=1} onClick={()=>stepper(-1)}>‹</button>
      <span data-preview-record={record}>{tr('레코드 {0} / {1}',{0:record,1:records})}</span>
      <button type="button" aria-label={tr('다음 레코드')} disabled={record>=records} onClick={()=>stepper(1)}>›</button>
    </div>
    <fieldset className="datamerge-computed-edit" disabled={!ctx.editable}>
    {drafts.map((draft,i)=>{
      const shown=evaluated(i),column=applied.findIndex(a=>a.name===draft.name),field=catalogue.computed?.added.find(a=>a.computed===column);
      const error=draft.expr.trim()?shown?.definitionError??shown?.error:undefined;
      const brackets=bracketInfo(draft.expr,caret.row===i?caret.at:draft.expr.length);
      const kind=kindOfName(draft.name);
      return <div key={i} className="datamerge-computed-row" data-computed-field={draft.name}>
        <div className="datamerge-computed-head">
          <input aria-label={tr('계산 열 이름')} value={draft.name} spellCheck={false} onChange={e=>change(i,{name:e.target.value})}/>
          <button type="button" aria-label={tr('계산 열 삭제')} onClick={()=>setDrafts(d=>d.filter((_,k)=>k!==i))}>{tr('삭제')}</button>
        </div>
        <textarea ref={el=>{boxes.current[i]=el;}} aria-label={tr('계산 열 식')} value={draft.expr} rows={2} spellCheck={false}
          onFocus={()=>setFocused(i)} onChange={e=>{change(i,{expr:e.target.value});setCaret({row:i,at:e.target.selectionStart});}}
          onSelect={e=>setCaret({row:i,at:e.currentTarget.selectionStart})}/>
        <div className="datamerge-computed-tools">
          <select aria-label={tr('필드 넣기')} value="" onChange={e=>{if(e.target.value)put(i,e.target.value);}}>
            <option value="">{tr('필드 넣기…')}</option>
            {above(i).map((f,k,all)=><option key={k} value={fieldRef(f,all)}>{`${f.kind==='image'?'@':f.kind==='qrcode'?'#':''}${f.name}`}</option>)}
          </select>
          <button type="button" data-insert-computed="" disabled={!field||dirty||!canInsert(kind)} onMouseDown={e=>e.preventDefault()} onClick={()=>insertField(baseName(draft.name),kind)}>{tr('삽입')}</button>
        </div>
        {draft.expr.includes('(')||draft.expr.includes(')')?<small className="datamerge-brackets" data-brackets={brackets.balanced?'ok':'open'}>
          {!brackets.balanced?tr('괄호 짝이 맞지 않습니다: 여는 괄호 {0}개, 닫는 괄호 {1}개',{0:brackets.open,1:brackets.close})
            :brackets.partner!==null?tr('짝이 되는 괄호는 {0}번째 글자입니다.',{0:brackets.partner+1})
            :tr('괄호 {0}쌍이 맞습니다.',{0:brackets.open})}
        </small>:null}
        {draft.expr.trim()&&shown&&!error?<output className="datamerge-computed-value" data-computed-value={shown.value??''}>{shown.value?shown.value:tr('(빈 값)')}</output>:null}
        {error?<p className="datamerge-computed-error" role="alert" data-computed-error={error.code}>{errorWithPlace(error,true)}</p>:null}
      </div>;
    })}
    {evaluationError?<p role="alert">{evaluationError}</p>:null}
    <div className="datamerge-computed-actions">
      <button type="button" data-computed-add="" onClick={()=>setDrafts(d=>[...d,{name:newName(d.map(x=>x.name)),expr:''}])}>{tr('새 계산 열')}</button>
      <button type="button" data-computed-apply="" disabled={!dirty||!complete||failing||applying||!ctx.editable} onClick={()=>void apply()}>{applying?tr('적용하는 중…'):tr('적용')}</button>
      {dirty?<button type="button" onClick={()=>setDrafts(applied)}>{tr('되돌리기')}</button>:null}
    </div>
    </fieldset>
    {errorCount>0&&!dirty?<div className="datamerge-computed-report" data-record-errors={errorCount}>
      <p>{tr('식이 맞지 않은 레코드 {0}곳: 병합하면 그 칸은 비어 있습니다.',{0:errorCount})}</p>
      {errors.slice(0,5).map((e,k)=><small key={k}>{recordErrorText(e)}</small>)}
    </div>:null}
    <details className="datamerge-functions" data-function-help="">
      <summary>{tr('함수 목록')}</summary>
      <p>{tr('함수 이름은 대소문자를 가리지 않습니다. 눌러서 식에 넣으세요.')}</p>
      <div role="list">{functions.map(f=><div role="listitem" key={f.name} className="datamerge-function">
        <button type="button" data-function={f.name} disabled={!drafts.length} onMouseDown={e=>e.preventDefault()} onClick={()=>put(Math.min(focused,drafts.length-1),`${f.name}()`,1)}>{f.name}</button>
        <code>{f.syntax}</code>
        <span>{functionHelp(f.name)}</span>
      </div>)}</div>
      <p>{tr('TEXT 서식 예: #,##0 · 0.0 · 0% · yyyy년 m월 d일 · yyyy-mm-dd · aaa(한국어 요일)')}</p>
    </details>
  </fieldset>;
}

