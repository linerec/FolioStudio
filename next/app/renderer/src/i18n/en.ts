import menus from './menus.en';
import messages from './messages.en';
import metadata from './metadata.en';
import preflight from './preflight.en';
import toc from './toc.en';
import preferences from './preferences.en';
export const en = {
  ...menus,
  ...messages,
  ...metadata,
  ...preflight,
  ...toc,
  ...preferences,
} as const;
