import type { Settings } from '../../engine';
import type { Command, Commands } from '../../ui/commands';
import { sameShortcut } from '../../ui/keys';
import { SPECS } from '../../ui/menudef';

type Shortcuts = Settings['shortcuts'];
const overridableDefault = (id: string, command: Command, custom: Shortcuts) => command.fallThrough && custom?.[id] === undefined;

/** Explicit user shortcuts take precedence over contextual defaults, regardless of command order or selection. */
export function applyShortcuts(commands: Commands, custom: Shortcuts): Commands {
  const assigned = Object.entries(custom ?? {}).filter(([id]) => commands[id]).flatMap(([, keys]) => keys);
  for (const [id, command] of Object.entries(commands)) {
    const keys = custom?.[id] ?? (overridableDefault(id, command, custom)
      ? command.keys?.filter(key => !assigned.some(other => sameShortcut(key, other))) : command.keys);
    commands[id] = { ...command, keys };
  }
  return commands;
}

/** The commands the Keyboard Shortcuts dialog works on (the engine's dialog `shortcuts` takes them when it opens): each with its default keys
 *  (the menus' own), whether a contextual default gives way to a user's key, and whether the list offers it (it can run, or shows while it cannot). */
export const shortcutCommandList = (commands: Commands) => Object.entries(commands).map(([id, c]) => ({
  id, label: c.label, keys: SPECS[id]?.keys ?? [], fallThrough: !!c.fallThrough, listed: !!(c.run || c.shortcutVisible) && !/^(recent|docpreset)\./.test(id),
}));
