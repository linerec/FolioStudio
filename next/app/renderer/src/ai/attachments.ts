import type {Target} from '../print';
import type {Fields} from './runtime';
import {tr} from '../i18n';

export type Spec = Fields & {kind:string};
export type Ask = {specs:Spec[]; focus:boolean; replaceRegions:boolean};
export function askDocument(specs:Spec[], focus=true, replaceRegions=false) {
  window.dispatchEvent(new CustomEvent<Ask>('folio-ai-ask',{detail:{specs,focus,replaceRegions}}));
}

export type Snapshot = {
  kind:string; document?:string; page:number|null; region?:number[]; structureHash?:string;
  objects?:{id:string;name?:string;bounds:number[]}[];
  text?:{story:string;start:number;end:number;excerpt:string};
  file?:{file?:string;name?:string};style?:{name?:string;style?:{name?:string}};
  finding?:{rule:string;page:number|null;item:string|null;messageId?:string;messageArgs?:Fields};
  image?:{width:number;height:number;png?:string;sha256:string};
};
export function selector(s:Snapshot):Spec|undefined {
  const base={kind:s.kind,page:s.page??0,document:s.document};
  switch(s.kind){
    case 'objects':return {...base,ids:s.objects?.map(o=>o.id)};
    case 'region':return {...base,region:s.region};
    case 'page':return base;
    case 'text':return s.text && {...base,story:s.text.story,start:s.text.start,end:s.text.end};
    case 'preflight':return s.finding && {...base,finding:findingKey(s.finding)};
  }
}
export function findingKey(f:NonNullable<Snapshot['finding']>) {
  return {rule:f.rule,page:f.page,item:f.item,messageId:f.messageId,messageArgs:f.messageArgs};
}
export function chipLabel(s:Snapshot) {
  const page=tr('페이지 {0}',{0:(s.page??0)+1});
  switch(s.kind){
    case 'objects':return [page,...(s.objects??[]).map(o=>o.name||o.id)].join(' · ');
    case 'text':return [page,s.text?.excerpt.slice(0,32)||tr('글 범위')].join(' · ');
    case 'region':return tr('{0} · 영역 · 개체 {1}개',{0:page,1:s.objects?.length??0});
    case 'preflight':return s.finding?.rule??tr('사전 검사');
    case 'file':return s.file?.name??s.file?.file??tr('파일');
    case 'style':return s.style?.style?.name??s.style?.name??tr('스타일');
    default:return page;
  }
}
export function chipTarget(s:Snapshot):Target|null {
  return s.page==null || !selector(s) ? null : {page:s.page,bounds:s.region??null};
}
export type Marks={path:string;targets:Target[];text?:Snapshot['text']};
export function markDocument(path:string,targets:Target[],navigate=false,text?:Snapshot['text']) {
  window.dispatchEvent(new CustomEvent('folio-ai-mark',{detail:{path,targets,navigate,text}}));
}
