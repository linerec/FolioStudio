export type Profile = {name:string;disabled_rules:string[];parameters:Record<string,number>};
export type Profiles = {active:string;profiles:Profile[]};
export type Settings = {page_range:string;layers:string;include_pasteboard:boolean;include_nonprinting:boolean;off:boolean};
