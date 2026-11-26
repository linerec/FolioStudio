import {useEffect,useRef} from 'react';
import {tr} from '../../i18n';
import {useSelected,type PanelCtx} from '../../ui/panels';
import {NumberField} from '../../ui/controls/numberfield';
import {declarePanelCommands,usePanelCommands} from '../../ui/panelbus';
import type {DataMergeState} from './state';
import type {ImportOptions,MergeOptions} from './query';
import {ComputedFields,recordErrorText} from './computed';
import {DatabaseBlock} from './db/DatabaseBlock';
import {ExportBlock,PreviewBlock,useMergePreview} from './preview';
import {useMergedDocument} from './merged';
import './panel.css';

export const DATA_MERGE_PANELS={
  dataMerge:{get title(){return tr('데이터 병합');},icon:'table',width:340,height:440},
};
/** Data Source Import Options (InDesign's dialog after Show Import Options): what is not chosen is what the dialog fills in for the file. */
function ImportForm({state}:{state:DataMergeState}) {
  const o=state.importOptions,set=(patch:Partial<ImportOptions>)=>state.setImportOptions({...o,...patch});
  const applied=state.catalogue?.source.options;
  const names:Record<string,string>={comma:tr('쉼표'),tab:tr('탭 문자'),semicolon:tr('세미콜론'),ascii:'ASCII',unicode:tr('유니코드 (UTF-16)'),shiftJis:'Shift_JIS',pc:'PC',macintosh:'Macintosh'};
  return <fieldset className="datamerge-import" disabled={state.busy}>
    <legend>{tr('가져오기 옵션')}</legend>
    <label>{tr('구분자')}<select value={o.delimiter} onChange={e=>set({delimiter:e.target.value as ImportOptions['delimiter']})}>
      <option value="">{tr('자동')}</option><option value="comma">{tr('쉼표')}</option><option value="tab">{tr('탭 문자')}</option><option value="semicolon">{tr('세미콜론')}</option></select></label>
    <label>{tr('인코딩')}<select value={o.encoding} onChange={e=>set({encoding:e.target.value as ImportOptions['encoding']})}>
      <option value="">{tr('자동')}</option><option value="ascii">ASCII</option><option value="unicode">{tr('유니코드 (UTF-16)')}</option><option value="shiftJis">Shift_JIS</option></select></label>
    <label>{tr('플랫폼')}<select value={o.platform} onChange={e=>set({platform:e.target.value as ImportOptions['platform']})}>
      <option value="">PC</option><option value="macintosh">Macintosh</option></select></label>
    <label className="check"><input type="checkbox" checked={o.preserveSpaces} onChange={e=>set({preserveSpaces:e.target.checked})}/>{tr('데이터 원본의 공백 유지')}</label>
    {applied&&<p role="status" data-applied={`${applied.delimiter}/${applied.encoding}/${applied.platform}`}>{tr('읽은 옵션: {0}',{0:`${names[applied.delimiter]??applied.delimiter} · ${names[applied.encoding]??applied.encoding} · ${names[applied.platform]??applied.platform}`})}</p>}
  </fieldset>;
}
function MergeForm({state,onCreate}:{state:DataMergeState;onCreate:()=>void}) {
  const o=state.options,set=(patch:Partial<MergeOptions>)=>state.setOptions({...o,...patch});
  // lengths in points, 0 or more (no most, as before): a value is taken when Enter is pressed or the box is left
  const length=(label:string,value:number,change:(v:number)=>void)=><NumberField value={value} onCommit={change} min={0} max={1e9} step={1} decimals={4} label={label} width="100%"/>;
  const margin=(index:number,label:string)=><label key={index}>{label}{length(label,o.margins[index],v=>set({margins:o.margins.map((m,i)=>i===index?v:m) as MergeOptions['margins']}))}</label>;
  return <fieldset className="datamerge-merge" disabled={state.creating}>
    <legend>{tr('병합 문서 만들기')}</legend>
    <label>{tr('레코드 범위 (비우면 전체, 예: 2-5)')}<input aria-label={tr('레코드 범위')} value={o.range} onChange={e=>set({range:e.target.value})}/></label>
    <label className="check"><input type="checkbox" checked={o.multiple} onChange={e=>set({multiple:e.target.checked})}/>{tr('한 쪽에 레코드 여러 개')}</label>
    {o.multiple&&<>
      <label>{tr('배치')}<select value={o.arrange} onChange={e=>set({arrange:e.target.value as MergeOptions['arrange']})}>
        <option value="rows">{tr('가로 먼저')}</option><option value="columns">{tr('세로 먼저')}</option></select></label>
      <label>{tr('세로 간격 (pt)')}{length(tr('세로 간격 (pt)'),o.rowSpacing,v=>set({rowSpacing:v}))}</label>
      <label>{tr('가로 간격 (pt)')}{length(tr('가로 간격 (pt)'),o.columnSpacing,v=>set({columnSpacing:v}))}</label>
      {margin(0,tr('위쪽 여백 (pt)'))}{margin(1,tr('왼쪽 여백 (pt)'))}{margin(2,tr('아래쪽 여백 (pt)'))}{margin(3,tr('오른쪽 여백 (pt)'))}
    </>}
    <label>{tr('이미지 맞춤')}<select data-merge-fitting="" value={o.fitting} onChange={e=>set({fitting:e.target.value as MergeOptions['fitting']})}>
      <option value="proportional">{tr('비율에 맞춰 넣기')}</option><option value="fill">{tr('프레임 채우기')}</option>
      <option value="contentToFrame">{tr('그림을 프레임에 맞추기')}</option><option value="frameToContent">{tr('프레임을 그림에 맞추기')}</option><option value="none">{tr('크기 유지')}</option></select></label>
    {o.fitting==='none'&&<label className="check"><input type="checkbox" checked={o.centerImage} onChange={e=>set({centerImage:e.target.checked})}/>{tr('그림을 프레임 가운데에')}</label>}
    <label className="check"><input type="checkbox" checked={o.linkImages} onChange={e=>set({linkImages:e.target.checked})}/>{tr('이미지 링크 (끄면 문서에 넣음)')}</label>
    <label className="check"><input type="checkbox" checked={o.removeBlankLines} onChange={e=>set({removeBlankLines:e.target.checked})}/>{tr('빈 줄 제거')}</label>
    <button type="button" onClick={onCreate}>{state.creating?tr('만드는 중…'):tr('병합 문서 만들기')}</button>
    {state.createError&&<p role="alert">{tr('병합 문서를 만들지 못했습니다: {0}',{0:state.createError})}</p>}
    {state.created&&<p role="status" data-merged={state.created.documentId}>{tr('병합 문서를 만들었습니다: {0} ({1}쪽)',{0:state.created.relative??state.created.path,1:state.created.pages})}</p>}
    {!!state.created?.missing.length&&<p role="status">{tr('찾지 못한 이미지는 빈 프레임으로 남았습니다: {0}',{0:state.created.missing.join(', ')})}</p>}
    {!!state.created?.computedErrorCount&&<div role="status" data-computed-errors={state.created.computedErrorCount}>
      <p>{tr('계산 열의 식이 맞지 않은 레코드가 {0}곳 있습니다. 그 칸은 비었습니다.',{0:state.created.computedErrorCount})}</p>
      {(state.created.computedErrors??[]).slice(0,10).map((e,k)=><small key={k}>{recordErrorText(e)}</small>)}
    </div>}
  </fieldset>;
}
/** What the document remembers about its data source: the file, and whether it is still the one that was selected (compared by content, not by date). */
function LinkNotice({state,editable,onRefresh,onRemove}:{state:DataMergeState;editable:boolean;onRefresh:()=>void;onRemove:()=>void}) {
  const link=state.link;
  if(!link)return null;
  return <div className="datamerge-link" data-link-state={link.state}>
    {link.state==='ok'&&<p role="status">{tr('문서에 저장된 데이터 원본: {0}',{0:link.path??''})}</p>}
    {link.state==='changed'&&<p role="alert">{tr('데이터 원본 {0}의 내용이 선택했을 때와 달라졌습니다. 지금 파일의 내용을 보여 줍니다.',{0:link.path??''})}</p>}
    {link.state==='moved'&&<p role="alert">{tr('데이터 원본을 프로젝트 안의 다른 곳에서 찾았습니다: {0}',{0:link.path??''})}</p>}
    {link.state==='missing'&&<p role="alert">{tr('문서에 저장된 데이터 원본 파일을 프로젝트에서 찾을 수 없습니다.')}</p>}
    {editable&&<div className="datamerge-link-actions">
      {link.state!=='missing'&&<button type="button" disabled={state.busy} onClick={onRefresh}>{tr('데이터 원본 갱신')}</button>}
      <button type="button" onClick={onRemove}>{tr('데이터 원본 제거')}</button>
    </div>}
  </div>;
}
// The panel menu (menuaudit-c): what each item does, by command id.
const DM = 'id.panel.dataMerge.';
declarePanelCommands([DM+'selectDataSource', DM+'updateDataSource', DM+'removeDataSource', DM+'createMergedDocument', DM+'contentPlacementOptions', 'dataMerge.connectDatabase', DM+'preview', DM+'exportToPdf', DM+'updateContentInDataFields', DM+'showLogOfUpdateResults']);

export function DataMergePanel({ctx}:{ctx:PanelCtx}) {
  const state=ctx.dataMerge,selection=ctx.textFormatting.selection,db=ctx.databaseSource;
  // the document's data source is a database: the snapshot it keeps is read like a file, but the file's own controls (path, import options) do not apply
  const fromDatabase=!!db?.status;
  const selected=useSelected(ctx);
  // the data source the document remembers: read when the document opens and whenever the stored reference changes (select, remove, undo, redo)
  const linkKey=ctx.info?.dataMergeSource?.file??'';
  // the computed fields are fields of the source: it is read again when their list changes (apply, undo, redo)
  const computedKey=JSON.stringify(ctx.info?.dataMergeComputed??[]);
  useEffect(()=>{if(state?.available)void state.loadLinked();},[ctx.doc?.path,linkKey,computedKey,state?.available]);
  const imageTarget=ctx.selection.length===1&&selected&&['rectangle','oval','polygon'].includes(selected.item.kind)&&!selected.item.content&&!selected.item.locked&&!selected.entry.content?selected.item.id:null;
  const pathInput=useRef<HTMLInputElement>(null),mergeBox=useRef<HTMLDivElement>(null),pdfInput=useRef<HTMLInputElement>(null);
  // Preview: the document shows one record in its placeholders; it ends before anything reads the document (create, export)
  const preview=useMergePreview(state,ctx);
  // a merged document (build-c2): Update Content in Data Fields and its log
  const merged=useMergedDocument(ctx);
  const createMerged=async()=>{await preview.stop();await state?.create();};
  // Select Data Source: the file is read, then the document remembers it (project-relative path and SHA-256, one undo step)
  const choose=async()=>{
    if(!state)return;
    const found=await state.read();
    const o=state.importOptions;
    if(found&&ctx.editable)await ctx.edit(tr('데이터 원본 선택'),[{op:'selectDataSource',path:found.path,source:undefined,file_type:undefined,delimiter:o.delimiter||undefined,encoding:o.encoding||undefined,platform:o.platform||undefined,preserve_spaces:o.preserveSpaces||undefined}]);
  };
  const editable=ctx.editable,linked=state?.link&&state.link.state!=='missing';
  // a field goes into the document where the selection is: text at the caret, an image or QR code field into the selected empty frame
  const canInsert=(kind:string)=>ctx.editable&&(kind==='text'?!!selection&&selection.start===selection.end&&!selection.nested:!!imageTarget);
  const insert=(name:string,kind:string)=>{
    if(kind==='image'&&imageTarget)void ctx.edit(tr('데이터 병합 필드 삽입'),[{op:'insertDataMergeImageField',ids:[imageTarget],field:name}]);
    else if(kind==='qrcode'&&imageTarget)void ctx.edit(tr('데이터 병합 필드 삽입'),[{op:'insertDataMergeQrField',ids:[imageTarget],field:name}]);
    else if(kind==='text'&&selection)void ctx.edit(tr('데이터 병합 필드 삽입'),[{op:'insertDataMergeTextField',story:selection.story,at:selection.start,field:name}]);
  };
  usePanelCommands('dataMerge',{
    // Select Data Source...: with a path typed, the button's work; otherwise the path box gets the focus
    [DM+'selectDataSource']:state?.available&&!state.busy?(state.path.trim()?()=>void choose():()=>pathInput.current?.focus()):undefined,
    [DM+'updateDataSource']:editable&&linked&&!state?.busy?()=>void choose():undefined,
    [DM+'removeDataSource']:editable&&state?.link?()=>void ctx.edit(tr('데이터 원본 제거'),[{op:'removeDataSource'}]):undefined,
    [DM+'createMergedDocument']:state?.catalogue&&!state.creating?()=>void createMerged():undefined,
    [DM+'preview']:state?.catalogue&&preview.available&&state.catalogue.source.records.length?{run:()=>{if(state.mergePreview)void preview.stop();else state.setMergePreview(true);},checked:state.mergePreview}:undefined,
    [DM+'exportToPdf']:state?.catalogue&&!state.exporting?()=>pdfInput.current?.focus():undefined,
    [DM+'contentPlacementOptions']:state?.catalogue?()=>mergeBox.current?.querySelector<HTMLElement>('[data-merge-fitting]')?.focus():undefined,
    // Connect to Database…: choose a connection, a table and conditions, then import (features/datamerge/db)
    ['dataMerge.connectDatabase']:db?.available?()=>db.connect():undefined,
    [DM+'updateContentInDataFields']:merged.update,
    [DM+'showLogOfUpdateResults']:merged.showLog,
  });
  if(!state?.available)return <p className="p-empty">{tr('데이터 병합을 사용하려면 프로젝트 문서를 여세요.')}</p>;
  return <div className="datamerge-panel">
    {merged.block}
    {db&&<DatabaseBlock db={db}/>}
    {!fromDatabase&&<>
      <form onSubmit={e=>{e.preventDefault();void choose();}}>
        <label>{tr('프로젝트 상대 CSV/TXT 경로')}<input ref={pathInput} aria-label={tr('프로젝트 상대 CSV/TXT 경로')} value={state.path} disabled={state.busy} onChange={e=>state.setPath(e.target.value)}/></label>
        <button disabled={state.busy||!state.path.trim()}>{state.busy?tr('읽는 중…'):tr('데이터 원본 선택')}</button>
      </form>
      <ImportForm state={state}/>
      <p>{tr('한글 원본은 UTF-16으로 저장하세요. 원본의 첫 줄을 필드 이름으로 읽습니다.')}</p>
    </>}
    {state.error&&<p role="alert">{tr('데이터 원본을 읽지 못했습니다: {0}',{0:state.error})}</p>}
    {state.link&&!fromDatabase&&<LinkNotice state={state} editable={ctx.editable} onRefresh={()=>void choose()} onRemove={()=>void ctx.edit(tr('데이터 원본 제거'),[{op:'removeDataSource'}])}/>}
    {fromDatabase&&ctx.editable&&<div className="datamerge-link-actions"><button type="button" data-db-remove-source onClick={()=>void ctx.edit(tr('데이터 원본 제거'),[{op:'removeDataSource'}])}>{tr('데이터 원본 제거')}</button></div>}
    {state.catalogue&&<>
      {!fromDatabase&&<p>{state.catalogue.path}</p>}
      <p>{tr('필드 {0}개 · 레코드 {1}개',{0:state.catalogue.source.fields.length-(state.catalogue.computed?.added.length??0),1:state.catalogue.source.records.length})}</p>
      <p>{tr('글 안에 커서를 놓고 텍스트 필드를 삽입하세요.')}</p>
      <p>{tr('이미지 필드와 QR 코드 필드는 빈 사각형·타원·다각형을 선택한 뒤 삽입하세요.')}</p>
      <ul>{state.catalogue.source.fields.filter((_,index)=>!state.catalogue?.computed?.added.some(a=>a.column===index)).map((field,index)=><li key={index} data-field={field.name}>
        <span>{field.kind==='image'?'@':field.kind==='qrcode'?'#':''}{field.name}</span>
        <button type="button" disabled={!canInsert(field.kind)} onMouseDown={e=>e.preventDefault()} onClick={()=>insert(field.name,field.kind)}>{tr('삽입')}</button>
      </li>)}</ul>
      <PreviewBlock state={state} preview={preview}/>
      <ComputedFields state={state} ctx={ctx} canInsert={canInsert} insertField={insert}/>
      <div ref={mergeBox}><MergeForm state={state} onCreate={()=>void createMerged()}/></div>
      <ExportBlock state={state} ctx={ctx} preview={preview} pathInput={pdfInput}/>
    </>}
  </div>;
}
