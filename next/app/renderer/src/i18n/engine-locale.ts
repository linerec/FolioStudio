import { currentLanguage, subscribeLanguage } from './index';
import english from './engine-messages.en';

/**
 * The window's preload turns the engine's failure answers into the screen's language (`electron/engine-message.cjs`); it learns the
 * language, and the English sentences, here. Korean needs nothing: the engine's own text is Korean.
 */
export function followLanguageForEngineMessages(): void {
  const push = () => window.folio.setEngineLocale?.(currentLanguage(), currentLanguage() === 'ko' ? {} : english);
  push();
  subscribeLanguage(push);
}
