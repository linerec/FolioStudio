import type { InksReport } from '../engine';
import type { Command } from './commands';

/** Use the same plate order and single-plate operation as the Separations panel. */
export function spotKeyCommands(report: InksReport | null, solo: (name: string) => void): Record<string, Partial<Command>> {
  const spots = report?.inks.filter(ink => ink.kind === 'spot') ?? [];
  return Object.fromEntries(Array.from({ length: 5 }, (_, i) => [`sep.key.spot${i + 1}`, {
    shortcutVisible: true,
    run: spots[i] ? () => solo(spots[i].name) : undefined,
  }]));
}
