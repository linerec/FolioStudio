import {useEffect,useState} from 'react';
import type {DocumentSummary} from '../../engine';

import {type Filters,NO_FILTERS} from './rules';
export type {Filters};
export {NO_FILTERS};
/** A warning of File ▸ Import PDF Comments… (InDesign's alert) or the question about a PDF from outside the project. */
export type ImportAlert={kind:'noComments'|'corrupt'|'notInDesign'|'otherDocument'|'duplicate'|'copy'; path:string};

/** What the PDF Comments panel remembers while the document is open: the selected comment, search and filters, the notice after an import. */
export function usePdfComments(doc:DocumentSummary|null) {
  const [selected,setSelected]=useState<string|null>(null);
  const [search,setSearch]=useState('');
  const [filters,setFilters]=useState<Filters>(NO_FILTERS);
  const [toast,setToast]=useState('');
  const [alert,setAlert]=useState<ImportAlert|null>(null);
  /** The eye button: hides what InDesign draws on the layout for the comments. Per open document and only while it is open (measured: a second
   *  document opened next to it is shown, the first stays hidden when it is shown again, and a saved copy opens shown). */
  const [hidden,setHidden]=useState(false);
  useEffect(()=>{setSelected(null);setSearch('');setFilters(NO_FILTERS);setToast('');setAlert(null);setHidden(false);},[doc?.path]);
  return {available:!!doc?.folio,documentPages:doc?.pages.length??0,selected,setSelected,search,setSearch,filters,setFilters,toast,setToast,alert,setAlert,hidden,setHidden};
}
export type PdfCommentsState=ReturnType<typeof usePdfComments>&{
  /** File ▸ Import PDF Comments…: pick a PDF (the project's), tie its comments to the layout and store them in the document. */
  start?:()=>void;
  /** Picking a comment on text selects that text in the document (InDesign: the selection becomes the commented word). */
  reveal?:(range:{story:string;start:number;end:number})=>void;
};
