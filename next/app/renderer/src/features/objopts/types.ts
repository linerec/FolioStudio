export type FrameFitting = {autoFit:boolean;topCrop:number;bottomCrop:number;leftCrop:number;rightCrop:number;fittingAlignment:string;fittingOnEmptyFrame:string};
export type ClippingOptions={kind:string;threshold:number;tolerance:number;inset:number;invert:boolean;insideEdges:boolean;restrictToFrame:boolean;highResolution:boolean;pathName:string;index:number};
/** A placed raster's source profile and rendering intent (Object ▸ Image Color Settings, the shared dialog `imageColor`). */
export type ImageColor = { profile: string; intent: string };
/** A placed picture's layer options (Object ▸ Object Layer Options, the shared dialog `objectLayerOptions`). */
export type GraphicLayers={update_link:string;layers:{id:number;name:string;original:boolean;visible:boolean}[];applied_comp?:number;comps?:{id:number;name:string;shown:number[]}[]};
