import menus from './menus.ko';
import legacy from './legacy.ko';
import metadata from './metadata.ko';
import preflight from './preflight.ko';
export const ko = {
  ...menus,
  ...legacy,
  ...metadata,
  ...preflight,
} as const;
