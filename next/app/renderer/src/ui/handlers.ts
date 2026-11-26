import { useRef, type RefObject } from 'react';
import type { Command, Commands } from './commands';

export type Handlers = Record<string, Partial<Command>>;

/** Ordered registrations: later groups replace earlier entries, just like object spreads. */
export function useHandlerRegistry(groups: readonly Handlers[], complete: (handlers: Handlers) => Commands): RefObject<Commands> {
  const registry = useRef<Commands>({});
  // Keep both metadata and closures from this render. Consumers retain only the ref.
  const handlers: Handlers = Object.assign({}, ...groups);
  registry.current = complete(handlers);
  return registry;
}
