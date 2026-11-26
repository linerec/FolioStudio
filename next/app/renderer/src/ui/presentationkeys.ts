import { commandFor, type Command, type Commands } from './commands';

export type PresentationBackground = 'black' | 'gray' | 'white';

export function presentationKeyCommands(active: boolean, set: (color: PresentationBackground) => void): Record<string, Partial<Command>> {
  return Object.fromEntries((['black', 'gray', 'white'] as const).map(color => ['view.key.background.' + color, {
    fallThrough: true, shortcutVisible: true, run: active ? () => set(color) : undefined,
  }]));
}

export function runPresentationKey(e: KeyboardEvent, commands: Commands): boolean {
  const local = Object.fromEntries(Object.entries(commands).filter(([id, c]) => id.startsWith('view.key.background.') && c.run));
  const id = commandFor(e, local);
  if (!id) return false;
  local[id].run?.();
  return true;
}
