import type {DocumentInfo, ItemInfo, ItemStore} from '../inspect';
import type {TreeEntry} from '../project';
import {tr} from '../i18n';
import type {Reference} from './runtime';

export async function documentReferences(info:DocumentInfo|null,store:ItemStore,files:TreeEntry[]):Promise<Reference[]> {
  const out:Reference[]=[];
  function file(entry:TreeEntry) {
    if(entry.kind==='dir') entry.children.forEach(file);
    else out.push({label:entry.path,spec:{kind:'file',file:entry.path}});
  }
  files.forEach(file);
  if(!info)return out;
  for(const [kind,styles] of [['paragraph',info.paragraphStyles],['character',info.characterStyles],['object',info.objectStyles]] as const)
    for(const style of styles)out.push({label:style.name,spec:{kind:'style',styleKind:kind,style:style.id}});
  for(const swatch of info.swatches)out.push({label:tr('견본 {0}',{0:swatch.name})});
  for(const pg of info.pages){
    out.push({label:tr('페이지 {0}',{0:pg.index+1}),spec:{kind:'page',page:pg.index}});
    const data=await store.load(pg.index);
    function item(x:ItemInfo) {
      if(x.name)out.push({label:x.name,spec:{kind:'objects',page:pg.index,ids:[x.id]}});
      x.children.forEach(item);
    }
    data?.items.forEach(item);data?.masterItems.forEach(item);
  }
  return out;
}
