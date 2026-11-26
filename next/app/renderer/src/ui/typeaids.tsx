import { useEffect, useState } from 'react';
import { t as tr } from '../i18n';
import { newDocumentFonts } from '../project';
import type { Command } from './commands';
import type { TextFormatting } from './textformat';
import { TypeSizeDialog } from './typesize';
import { fontFamilyProperties } from './fontfamily';
import { TypeFontFindDialog, type FontFindContext } from './typefontfind';

export function useTypeAids(control: TextFormatting, context: FontFindContext) {
  const [sizeOpen, setSizeOpen] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  useEffect(() => { setSizeOpen(false); setFindOpen(false); }, [context.documentPath]);
  const pointSize = control.selection?.format.pointSize;
  const [installed, setInstalled] = useState<Record<string, string[]>>({});
  const [fontError, setFontError] = useState(false);
  useEffect(() => {
    let live = true;
    void newDocumentFonts().then(result => {
      if (live) setInstalled(Object.fromEntries(result.fonts.map(f => [f.family, f.styles.map(s => s.style)])));
    }).catch(() => { if (live) setFontError(true); });
    return () => { live = false; };
  }, []);
  const fonts = control.selection?.fonts ?? installed;
  const families = Object.keys(fonts).sort((a, b) => a.localeCompare(b));
  const family = String(control.selection?.format.appliedFont ?? '');
  const style = String(control.selection?.format.fontStyle ?? '');
  const handlers: Record<string, Partial<Command>> = {
    'id.type.size.other': { run: control.selection ? () => setSizeOpen(true) : undefined },
    'type.findFont': { run: context.documentPath ? () => setFindOpen(true) : undefined },
    'id.list.fonts': {
      label: fontError ? tr('글꼴 목록을 불러오지 못했습니다.') : tr('설치된 글꼴 없음'),
      items: families.length ? families.map(name => ({ cmd: `type.fontFamily.${name}` })) : undefined,
    },
  };
  for (const name of families) handlers[`type.fontFamily.${name}`] = {
    label: name, checked: name === family,
    run: control.selection ? () => control.apply(fontFamilyProperties(fonts, name, style)) : undefined,
  };
  return {
    handlers,
    dialog: <>
      {sizeOpen && <TypeSizeDialog initial={typeof pointSize === 'number' ? pointSize : undefined}
        apply={pointSize => control.apply({ pointSize })} close={() => setSizeOpen(false)} />}
      {findOpen && context.documentPath && <TypeFontFindDialog context={context} fonts={fonts} close={() => setFindOpen(false)} />}
    </>,
  };
}
