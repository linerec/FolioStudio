import { useState, type ReactNode } from 'react';
import { tr } from '../i18n';

function inline(text:string, review:(url:string)=>void): ReactNode[] {
  const tokens = text.split(/(!?\[[^\]\n]{1,500}\]\([^\s()]{1,2000}\)|\x60[^\x60\n]+\x60|\*\*[^*\n]+\*\*|\*[^*\n]+\*)/g);
  return tokens.map((s,index) => {
    const link = /^(!?)\[([^\]]+)\]\(([^)]+)\)$/.exec(s);
    if (link) {
      try {
        const url = new URL(link[3]);
        if (!link[1] && ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password)
          return <button className="ai-link" key={index} onClick={()=>review(url.href)}>{link[2]}</button>;
      } catch { /* Show malformed links as text. */ }
      return s;
    }
    if (index % 2 === 0) return s;
    if (s.startsWith('**')) return <strong key={index}>{s.slice(2,-2)}</strong>;
    if (s.startsWith('*')) return <em key={index}>{s.slice(1,-1)}</em>;
    return <code key={index}>{s.slice(1,-1)}</code>;
  });
}
export function Markdown({text}:{text:string}) {
  const [address, review] = useState('');
  const lines = text.split(/\r?\n/);
  const blocks:ReactNode[] = [];
  const list = /^(\s*[-*]|\s*\d+\.)\s+/;
  const fence = /^\x60{3}/;
  const cells = (line:string)=>line.trim().replace(/^\||\|$/g,'').split('|').map(x=>x.trim());
  for (let i=0;i<lines.length;) {
    const start=i, line=lines[i++];
    if (!line.trim()) continue;
    if (fence.test(line)) {
      const code:string[] = [];
      while (i<lines.length && !fence.test(lines[i])) code.push(lines[i++]);
      if (i<lines.length) i++;
      blocks.push(<pre key={start}><code>{code.join('\n')}</code></pre>);
    } else if (list.test(line)) {
      const ordered=/^\s*\d/.test(line), rows=[line];
      while (i<lines.length && list.test(lines[i]) && /^\s*\d/.test(lines[i])===ordered) rows.push(lines[i++]);
      const List = ordered ? 'ol' : 'ul';
      blocks.push(<List key={start}>{rows.map((r,k)=><li key={k}>{inline(r.replace(list,''),review)}</li>)}</List>);
    } else if (line.includes('|') && i<lines.length && cells(lines[i]).every(c=>/^:?-{3,}:?$/.test(c))) {
      const header=cells(line), rows:string[][]=[];
      i++;
      while (i<lines.length && lines[i].includes('|')) rows.push(cells(lines[i++]));
      blocks.push(<div className="ai-table" key={start}><table><thead><tr>{header.map((c,k)=><th key={k}>{inline(c,review)}</th>)}</tr></thead>
        <tbody>{rows.map((r,k)=><tr key={k}>{r.map((c,j)=><td key={j}>{inline(c,review)}</td>)}</tr>)}</tbody></table></div>);
    } else {
      const paragraph=[line];
      while (i<lines.length && lines[i].trim() && !list.test(lines[i]) && !fence.test(lines[i]) && !lines[i].includes('|')) paragraph.push(lines[i++]);
      blocks.push(<p key={start}>{inline(paragraph.join('\n'),review)}</p>);
    }
  }
  return <>{blocks}{address && <aside className="ai-card"><code>{address}</code><button onClick={()=>{void window.folio.openExternal(address);review('');}}>{tr('브라우저에서 열기')}</button><button onClick={()=>review('')}>{tr('취소')}</button></aside>}</>;
}
