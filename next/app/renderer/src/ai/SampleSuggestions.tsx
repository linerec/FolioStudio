import { createContext, useContext, useEffect, useLayoutEffect, useRef, type ReactNode, type RefObject } from 'react';
import { tr } from '../i18n';
import { SampleCards, SampleContext } from '../ui/SampleCards';
import './sample-suggestions.css';

const DraftContext = createContext<RefObject<string> | null>(null);

export function SampleDraftScope({ opening, children }: { opening: boolean; children: ReactNode }) {
  const draft = useRef('');
  useEffect(() => {
    if (!opening) draft.current = '';
  }, [opening]);
  return <DraftContext.Provider value={draft}>{children}</DraftContext.Provider>;
}

// Carry only the text while the new document loads its own AI session and attachments.
export function useSampleDraft(path: string | undefined, value: string, setValue: (text: string) => void) {
  const draft = useContext(DraftContext);
  const opening = useContext(SampleContext)?.openingSample;
  useLayoutEffect(() => {
    if (!draft) return;
    if (!path && opening) draft.current = value;
    if (path && draft.current) {
      setValue(draft.current);
      draft.current = '';
    }
  }, [path, value, setValue, draft, opening]);
}

export function SampleSuggestions({ choose }: { choose: (text: string) => void }) {
  const questions = [tr('이 잡지의 표지 제목을 더 짧게 바꿔 줘'), tr('이 표본의 사전 검사 오류를 설명해 줘')];
  return <section className="ai-sample-suggestions" aria-label={tr('샘플로 해 보기')}>
    <h3>{tr('샘플로 해 보기')}</h3>
    <SampleCards compact />
    <div className="ai-sample-questions">
      {questions.map(question => <button type="button" key={question} onClick={() => choose(question)}>{question}</button>)}
    </div>
  </section>;
}
