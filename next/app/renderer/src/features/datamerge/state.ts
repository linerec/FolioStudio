import {useEffect,useRef,useState} from 'react';
import type {DocumentSummary} from '../../engine';
import {createMergedDocument,exportMergedPdf,NO_IMPORT_OPTIONS,readDataMergeSource,readLinkedDataMergeSource,type DataMergeSource,type ImportOptions,type MergedDocument,type MergedPdf,type MergeOptions} from './query';

export const DEFAULT_MERGE_OPTIONS:MergeOptions={range:'',multiple:false,arrange:'rows',rowSpacing:0,columnSpacing:0,margins:[36,36,36,36],removeBlankLines:false,linkImages:true,fitting:'proportional',centerImage:false};
/** What the document remembers about its data source and whether the file in the project is still that one. */
export type DataSourceLink={state:'ok'|'changed'|'moved'|'missing';path:string|null};

export function useDataMerge(doc:DocumentSummary|null) {
  const [path,setPath]=useState(''),[catalogue,setCatalogue]=useState<DataMergeSource|null>(null);
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const [options,setOptions]=useState<MergeOptions>(DEFAULT_MERGE_OPTIONS),[creating,setCreating]=useState(false);
  const [created,setCreated]=useState<MergedDocument|null>(null),[createError,setCreateError]=useState('');
  const [link,setLink]=useState<DataSourceLink|null>(null);
  const [importOptions,setImportOptions]=useState<ImportOptions>(NO_IMPORT_OPTIONS);
  /** The record (from 1) whose computed values the panel shows. */
  const [previewRecord,setPreviewRecord]=useState(1);
  const generation=useRef(0);
  useEffect(()=>{
    generation.current++;setPath('');setCatalogue(null);setBusy(false);setError('');setCreating(false);setCreated(null);setCreateError('');setLink(null);setImportOptions(NO_IMPORT_OPTIONS);setPreviewRecord(1);
    return ()=>{generation.current++;};
  },[doc?.path]);
  const read=async():Promise<DataMergeSource|null>=>{
    const ticket=++generation.current;
    setBusy(true);setError('');setCatalogue(null);
    try {const result=await readDataMergeSource(path,importOptions);if(ticket===generation.current){setCatalogue(result);return result;}}
    catch(e){if(ticket===generation.current)setError(String(e));}
    finally{if(ticket===generation.current)setBusy(false);}
    return null;
  };
  /** The data source the document remembers (Data Merge ▸ Select Data Source, kept in the document): its fields and records, and whether the file changed. */
  const loadLinked=async()=>{
    const ticket=++generation.current;
    try {
      const result=await readLinkedDataMergeSource();
      if(ticket!==generation.current)return;
      if(!result.linked){setLink(null);return;}
      setLink({state:result.state,path:result.path});
      setImportOptions({...NO_IMPORT_OPTIONS,...(result.importOptions??{})});
      setError('');
      if(result.path&&result.source&&result.sha256){setPath(result.path);setCatalogue({path:result.path,sha256:result.sha256,source:result.source,computed:result.computed});}
      else setCatalogue(null);
    } catch(e){if(ticket===generation.current)setError(String(e));}
  };
  const create=async()=>{
    if(!catalogue)return;
    const ticket=generation.current;
    setCreating(true);setCreated(null);setCreateError('');
    try {const result=await createMergedDocument(catalogue.path,options,importOptions);if(ticket===generation.current)setCreated(result);}
    catch(e){if(ticket===generation.current)setCreateError(String(e));}
    finally{if(ticket===generation.current)setCreating(false);}
  };
  /** The source again, as the document has it now (after the computed fields changed): the remembered one, else the file that was read. */
  const reload=()=>{if(link)void loadLinked();else if(path.trim()&&catalogue)void read();};
  // Data Merge ▸ Export to PDF: the merged records written straight to a project PDF
  // Data Merge ▸ Preview: the document shows the record `previewRecord` in its placeholders (features/datamerge/preview.tsx)
  const [mergePreview,setMergePreview]=useState(false);
  useEffect(()=>{setMergePreview(false);},[doc?.path]);
  const [pdfPath,setPdfPath]=useState(''),[exporting,setExporting]=useState(false);
  const [exported,setExported]=useState<MergedPdf|null>(null),[exportError,setExportError]=useState('');
  useEffect(()=>{setPdfPath('');setExporting(false);setExported(null);setExportError('');},[doc?.path]);
  const exportPdf=async(output:string)=>{
    if(!catalogue||!output.trim())return;
    const ticket=generation.current;
    setExporting(true);setExported(null);setExportError('');
    try {const result=await exportMergedPdf(catalogue.path,output.trim(),options,importOptions);if(ticket===generation.current)setExported(result);}
    catch(e){if(ticket===generation.current)setExportError(String(e));}
    finally{if(ticket===generation.current)setExporting(false);}
  };
  return {path,setPath,catalogue,busy,error,read,link,loadLinked,reload,importOptions,setImportOptions,previewRecord,setPreviewRecord,available:!!doc?.folio,options,setOptions,creating,created,createError,create,
    mergePreview,setMergePreview,pdfPath,setPdfPath,exporting,exported,exportError,exportPdf};
}
export type DataMergeState=ReturnType<typeof useDataMerge>;
