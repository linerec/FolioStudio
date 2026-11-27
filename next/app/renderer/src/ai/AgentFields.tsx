import { tr } from '../i18n';
import { Markdown } from './Markdown';

const adapters = [
  {name:'Claude Code', command:'claude-agent-acp', args:[], url:'https://github.com/agentclientprotocol/claude-agent-acp'},
  {name:'Codex', command:'codex-acp', args:[], url:'https://github.com/agentclientprotocol/codex-acp'},
  {name:'Gemini CLI', command:'gemini', args:['--acp'], url:'https://github.com/google-gemini/gemini-cli'},
];
type Props = {values:Record<string,unknown>; change:(key:string,value:string)=>void};
export function AgentFields({values,change}:Props) {
  const selected=adapters.find(a=>a.command===values.command);
  return <>
    <label>{tr('에이전트 선택')}<select value={selected?.command??''} onChange={e=>{
      const adapter=adapters.find(a=>a.command===e.target.value);
      change('command',adapter?.command??''); change('args',JSON.stringify(adapter?.args??[]));
    }}><option value="">{tr('직접 입력')}</option>{adapters.map(a=><option key={a.command} value={a.command}>{a.name}</option>)}</select></label>
    <label>{tr('실행 명령')}<input data-field="command" value={String(values.command??'')} onChange={e=>change('command',e.target.value)}/></label>
    <label>{tr('인수 (JSON 배열)')}<input data-field="args" value={String(values.args??'[]')} onChange={e=>change('args',e.target.value)}/></label>
    <label>{tr('환경 변수 (JSON 객체)')}<input data-field="env" value={String(values.env??'{}')} onChange={e=>change('env',e.target.value)}/></label>
    <small>{tr('키 값에는 ${credential}을 쓰세요. LANG, LC_ALL, TZ는 직접 입력할 수 있습니다.')}</small>
    {selected && <Markdown text={`[${tr('설치 안내')}](${selected.url})`}/>}
  </>;
}
