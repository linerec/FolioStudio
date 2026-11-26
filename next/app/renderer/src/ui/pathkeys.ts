import { commandFor, type Commands, type Command } from './commands';

export function pathKeyCommands(finish: (() => void) | null): Record<string, Partial<Command>> {
  return { 'object.key.endPath': { fallThrough: true, shortcutVisible: true, run: finish ?? undefined } };
}

export function runPathKey(e: KeyboardEvent, commands: Commands): boolean {
  const c = commands['object.key.endPath'];
  if (!c?.run || !commandFor(e, { 'object.key.endPath': c })) return false;
  c.run();
  return true;
}
