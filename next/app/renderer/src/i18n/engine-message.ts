import { currentLanguage } from './index';
import english from './engine-messages.en';

export type EngineMessage = {
  message?: string;
  messageId?: string;
  messageArgs?: Record<string, unknown>;
};

/** Only explicit message objects are translated. Plain argument strings are user data. */
export function engineMessage(value: EngineMessage): string {
  if (currentLanguage() === 'ko') return value.message ?? '';
  const render = (value: EngineMessage, depth: number): string => {
    const template = value.messageId && Object.hasOwn(english, value.messageId) ? english[value.messageId] : undefined;
    if (template === undefined || depth > 16) return value.message ?? value.messageId ?? '';
    const argument = (arg: unknown): string => {
      if (arg == null) return '';
      if (Array.isArray(arg)) return arg.map(argument).join(', ');
      if (typeof arg === 'object') return render(arg as EngineMessage, depth + 1);
      return String(arg);
    };
    return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
      Object.hasOwn(value.messageArgs ?? {}, key) ? argument(value.messageArgs![key]) : whole);
  };
  return render(value, 0);
}
