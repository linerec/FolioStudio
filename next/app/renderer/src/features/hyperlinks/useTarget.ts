import { useEffect, useState } from 'react';
import type { HyperlinkData } from './types';
import { parseTarget, type GotoTarget } from './goto';

/** The structured target of a link (engine query `hyperlinkTarget`), once resolved for this document state; undefined while unknown or unusable. */
export function useTarget(data: HyperlinkData, id: string | undefined): GotoTarget | undefined {
  const [resolved, setResolved] = useState<{ data: HyperlinkData; id: string; target: GotoTarget }>();
  useEffect(() => {
    let current = true;
    if (id) void window.folio.request({ cmd: 'hyperlinkTarget', link: id }).then(({ header }) => {
      const target = header.ok ? parseTarget(header.target) : undefined;
      if (current && target) setResolved({ data, id, target });
    }).catch(() => { /* An unavailable target keeps navigation disabled. */ });
    return () => { current = false; };
  }, [data, id]);
  return resolved?.data === data && resolved.id === id ? resolved.target : undefined;
}
