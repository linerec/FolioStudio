import type {ItemInfo, PageItems} from '../inspect';
import type {Target} from '../print';

type ChangedTarget = Target & {itemId?:string};

// A text edit reports a story ID. Every frame in that story can have reflowed.
export async function changeTargets(
  pages:readonly {index:number;id?:string|null}[],
  ids:readonly string[],
  load:(page:number)=>Promise<PageItems|null|undefined>,
  active:()=>boolean=()=>true,
):Promise<ChangedTarget[]> {
  const remaining=new Set(ids);
  const targets:ChangedTarget[]=[];
  for(const page of pages){
    if(!active()||!remaining.size)break;
    if(page.id && remaining.delete(page.id))targets.push({page:page.index,bounds:null});
    if(!remaining.size)break;
    const items=await load(page.index);
    if(!items)continue;
    const visit=(list:ItemInfo[])=>{
      for(const item of list){
        if(remaining.delete(item.id) || (item.text && remaining.has(item.text.story)))
          targets.push({page:page.index,bounds:item.bounds,itemId:item.id});
        visit(item.children);
      }
    };
    visit(items.items);
    visit(items.masterItems);
  }
  return targets;
}
