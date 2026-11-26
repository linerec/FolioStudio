export type HyperlinkData = {
  links: {id:string;name:string;source:string;destination:string}[];
  sources: {id:string;story:string;start:number;end:number;format:string}[];
  /** Sources that are page items: `item` is the frame (IDML SourcePageItem). */
  itemSources?: {id:string;item:string}[];
  destinations: {id:string;name:string;kind:string;value:string;story:string;at:number}[];
  formats: {id:string;name:string;characterStyle:string;blocks:{kind:string;text:string;characterStyle:string;delimiter:string;includeDelimiter:boolean}[]}[];
};
export type Link = HyperlinkData['links'][number];
export const emptyLinks:HyperlinkData={links:[],sources:[],destinations:[],formats:[]};
