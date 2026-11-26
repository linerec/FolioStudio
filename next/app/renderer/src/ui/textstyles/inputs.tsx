import { t as tr } from '../../i18n';
import type { StyleField } from './fields';
import { TextNumberField, formatNumeric, parseNumeric } from '../controls/lengthfield';

export type StyleValues = Record<string, string | number | boolean | null>;
export function StyleInputs({fields, values, set}: {fields:StyleField[]; values:StyleValues; set:(key:string,value:string|number|boolean)=>void}) {
  return <>{fields.map(f=><label key={f.key}>{f.label}{f.choices
    ?<select aria-label={tr('스타일 {0}',{0:f.label})} value={String(values[f.key]??'')} onChange={e=>set(f.key,f.boolean?e.target.value==='true':e.target.value)}>
      <option value="" disabled>{values[f.key]===null?tr('혼합'):tr('(상속)')}</option>
      {values[f.key]!=null&&!f.choices.some(c=>c.value===String(values[f.key]))&&<option value={String(values[f.key])} disabled>{String(values[f.key])}</option>}
      {f.choices.map(c=><option key={c.value} value={c.value} disabled={c.disabled}>{c.label}</option>)}
    </select>
    :f.text
      ?<input aria-label={tr('스타일 {0}',{0:f.label})} placeholder={values[f.key]===null?tr('혼합'):tr('(상속)')} value={String(values[f.key]??'')} onChange={e=>set(f.key,e.target.value)}/>
      :<TextNumberField label={tr('스타일 {0}',{0:f.label})} placeholder={values[f.key]===null?tr('혼합'):tr('(상속)')} text={String(values[f.key]??'')} onText={t=>set(f.key,t)}
        format={formatNumeric} parse={parseNumeric}
        word={f.key==='leading'?{text:tr('자동'),words:['auto',tr('자동').toLowerCase()],active:/^(auto|자동)$/i.test(String(values[f.key]??'').trim()),onChoose:()=>set('leading','auto')}:undefined}/>}</label>)}</>;
}

export function numericStyleValues(fields:StyleField[], dirty:Record<string,string|number|boolean>):Record<string,string|number|boolean>|null {
  const props={...dirty};
  for(const field of fields.filter(f=>f.numericChoice)) {
    const value=props[field.key];
    if(value!==undefined&&String(value).trim()&&Number.isFinite(Number(value)))props[field.key]=Number(value);
  }
  for(const field of fields.filter(f=>!f.text&&!f.choices)) {
    if(!(field.key in props))continue;
    const value=String(props[field.key]).trim();
    if(field.key==='leading'&&/^(auto|자동)$/i.test(value))props[field.key]='auto';
    else if(value&&Number.isFinite(Number(value)))props[field.key]=Number(value);
    else return null;
  }
  return props;
}
