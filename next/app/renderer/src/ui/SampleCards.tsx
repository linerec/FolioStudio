import { createContext, useContext } from 'react';
import { tr } from '../i18n';
import { Icon } from './icons';
import './sample-cards.css';

export type SampleKind = NonNullable<Parameters<Window['folio']['sample']>[0]>;
type SampleCommands = { openSample: (kind: SampleKind) => Promise<void>; openingSample: SampleKind | null };
export const SampleContext = createContext<SampleCommands | null>(null);

export function SampleCards({ compact = false }: { compact?: boolean }) {
  const commands = useContext(SampleContext);
  if (!commands) return null;
  const samples = [
    { kind: 'magazine', icon: 'book', title: tr('잡지 샘플 열기'),
      detail: tr('물굽이 서간 · 20쪽 · 바로 편집할 수 있는 프로젝트') },
    { kind: 'atlas', icon: 'pages', title: tr('기능 표본 열기 (Atlas)'), detail: tr('51쪽 · 표본 181개'),
      note: tr('사전 검사 표본이 들어 있어 열면 오류 표시가 보입니다.') },
  ] as const;
  return <div className={compact ? 'sample-cards compact' : 'sample-cards wide'}>
    {samples.map(sample => {
      const busy = commands.openingSample === sample.kind;
      return <button key={sample.kind} type="button" className="sample-card" data-sample={sample.kind}
        disabled={commands.openingSample !== null} aria-busy={busy} onClick={() => void commands.openSample(sample.kind)}>
        <Icon name={sample.icon} size={compact ? 16 : 20} />
        <span className="sample-copy">
          <strong>{sample.title}</strong><span>{sample.detail}</span>
          {'note' in sample && <span>{sample.note}</span>}
          {busy && <span role="status">{tr('문서를 여는 중')}</span>}
        </span>
      </button>;
    })}
  </div>;
}
