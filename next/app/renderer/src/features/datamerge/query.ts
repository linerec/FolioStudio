/** The choices of the Data Source Import Options dialog; empty is what the dialog fills in by itself for the file. */
export type ImportOptions={delimiter:''|'comma'|'tab'|'semicolon';encoding:''|'ascii'|'unicode'|'shiftJis';platform:''|'pc'|'macintosh';preserveSpaces:boolean};
export const NO_IMPORT_OPTIONS:ImportOptions={delimiter:'',encoding:'',platform:'',preserveSpaces:false};
/** A computed field: a field an expression makes from the columns (the name starts with @ for an image field, # for a QR code field). */
export type ComputedField={name:string;expr:string};
/** Where and why an expression is wrong: `code` and `args` are stable (the screen translates them), `pos` and `end` count characters from 0. */
export type ExprError={code:string;pos:number;end:number;args:string[];message:string};
export type RecordError=ExprError&{record:number;field:string};
export type DefinitionError=ExprError&{field:string};
/** The computed fields of the source: the definitions, which of them are fields of `source` and in which column, and the errors. */
export type ComputedInfo={fields:ComputedField[];added:{computed:number;column:number}[];definitionErrors:DefinitionError[];recordErrors:RecordError[];errorCount:number};
export type DataMergeSource = {
  path:string; sha256:string;
  source:{encoding:string;delimiter:string;fields:{name:string;kind:string}[];records:string[][];options?:{delimiter:string;encoding:string;platform:string;preserveSpaces:boolean}};
  /** The document's computed fields are fields of `source` (the last columns); `computed.added` says which. */
  computed?:ComputedInfo;
};
export async function readDataMergeSource(path:string,options:ImportOptions=NO_IMPORT_OPTIONS):Promise<DataMergeSource> {
  const {header}=await window.folio.request({cmd:'dataMergeSource',path,...options});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as DataMergeSource;
}
/** The data source the document remembers (`DataMerge` of its preferences: a project reference, root-relative path and SHA-256), read from the project:
 *  `ok` the file is what was selected, `changed` its content differs (the fields are those of the file now), `moved` the same content is elsewhere in the project, `missing`. */
export type LinkedDataMergeSource={linked:false;computed?:ComputedInfo}|{linked:true;path:string|null;state:'ok'|'changed'|'moved'|'missing';sha256?:string;storedSha256?:string|null;source?:DataMergeSource['source'];importOptions?:Partial<ImportOptions>|null;computed?:ComputedInfo};
export async function readLinkedDataMergeSource():Promise<LinkedDataMergeSource> {
  const {header}=await window.folio.request({cmd:'dataMergeSource'});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as LinkedDataMergeSource;
}
export type MergeOptions={range:string;multiple:boolean;arrange:'rows'|'columns';rowSpacing:number;columnSpacing:number;margins:[number,number,number,number];removeBlankLines:boolean;linkImages:boolean;fitting:'proportional'|'fill'|'contentToFrame'|'frameToContent'|'none';centerImage:boolean};
export type MergedDocument={documentId:string;path:string;relative:string|null;pages:number;missing:string[];computedErrors?:RecordError[];computedErrorCount?:number};
/** Data Merge ▸ Create Merged Document: the engine writes the merged document as a new .folio next to the open one. */
export async function createMergedDocument(path:string,options:MergeOptions,importOptions:ImportOptions=NO_IMPORT_OPTIONS):Promise<MergedDocument> {
  const {header}=await window.folio.request({cmd:'dataMergeCreate',path,...importOptions,...options});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as MergedDocument;
}
export type MergedPdf={path:string;pages:number;missing:string[];computedErrorCount?:number};
/** Data Merge ▸ Export to PDF: the engine merges the document with the source and writes the project-relative PDF `output` (no merged document is kept). */
export async function exportMergedPdf(path:string,output:string,options:MergeOptions,importOptions:ImportOptions=NO_IMPORT_OPTIONS):Promise<MergedPdf> {
  const {header}=await window.folio.request({cmd:'dataMergeExportPdf',path,output,...importOptions,...options});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as MergedPdf;
}
/** What the engine's expressions give for one record (`dataMergeEvaluate`): each field's value, or the error of its definition or of this record. */
export type EvaluatedField={name:string;expr:string;value?:string;error?:ExprError;definitionError?:ExprError};
export type Evaluation={fields:EvaluatedField[];record:number;recordCount:number;errorCount:number;definitionErrors:DefinitionError[];recordErrors:RecordError[]};
/** The values of `fields` for a record (1-based); the data source is the document's, or the project file `path` when none is remembered yet. */
export async function evaluateComputedFields(fields:ComputedField[],record:number,path?:string,options:ImportOptions=NO_IMPORT_OPTIONS):Promise<Evaluation> {
  const {header}=await window.folio.request({cmd:'dataMergeEvaluate',fields,record,...(path?{path,...options}:{})});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as Evaluation;
}
export type FunctionInfo={name:string;minArgs:number;maxArgs:number|null;syntax:string};
/** The functions the expression language has. */
export async function readFunctions():Promise<FunctionInfo[]> {
  const {header}=await window.folio.request({cmd:'dataMergeEvaluate',describe:true});
  if(!header.ok)throw Error(String(header.error));
  return (header as unknown as {functions:FunctionInfo[]}).functions;
}
