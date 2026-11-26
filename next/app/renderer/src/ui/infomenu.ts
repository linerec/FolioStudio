import { userSettings } from '../user-settings';
// Info panel menu: Show Options (the selection's kind, fill, stroke and placed file under the
// numbers) and Show Content Offset (a framed graphic's offset from its frame). This computer only.
import { useState } from 'react';
import type { Command } from './commands';

const KEY = 'folio.infoPanel';

export type InfoOptions = { options: boolean; contentOffset: boolean };

function stored(): InfoOptions {
  try {
    const v = JSON.parse(userSettings.getItem(KEY) ?? 'null');
    if (v && typeof v === 'object') return { options: v.options !== false, contentOffset: v.contentOffset === true };
  } catch { /* this computer only */ }
  return { options: true, contentOffset: false };
}

export function useInfoMenu(): { info: InfoOptions; handlers: Record<string, Partial<Command>> } {
  const [info, setInfo] = useState<InfoOptions>(stored);
  const toggle = (k: keyof InfoOptions) => () => setInfo((o) => {
    const n = { ...o, [k]: !o[k] };
    try { userSettings.setItem(KEY, JSON.stringify(n)); } catch { /* this computer only */ }
    return n;
  });
  return {
    info,
    handlers: {
      'id.panel.info.showOptions': { run: toggle('options'), checked: info.options },
      'id.panel.info.showContentOffset': { run: toggle('contentOffset'), checked: info.contentOffset },
    },
  };
}
