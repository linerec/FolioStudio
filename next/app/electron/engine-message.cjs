// The sentences the engine sends in Korean, in the language of the screen.
//
// A failure answer has `error` (Korean, unchanged) and, when the engine knows the sentence, `messageId` + `messageArgs`
// (next to `ok: false`). The window's preload gives every answer to `localize`, so each place that shows `header.error` shows it in the
// screen's language without asking. The English templates are those of `renderer/src/i18n/engine-messages.en.ts`, which the
// screen hands over with `setEngineLocale`. Arguments are user text (names, paths): they are put in as they are, never translated.
// `renderer/src/i18n/engine-message.ts` (`engineMessage`) renders the same way for the places that build a message themselves.

/** One message in English: its template with the arguments filled in; `fallback` (the Korean text) when there is no template. */
function render(english, fallback, id, args, depth = 0) {
  const template = id && Object.hasOwn(english, id) ? english[id] : undefined;
  if (template === undefined || depth > 16) return fallback ?? id ?? '';
  const argument = (value) => {
    if (value == null) return '';
    if (Array.isArray(value)) return value.map(argument).join(', ');
    if (typeof value === 'object') return render(english, value.message, value.messageId, value.messageArgs, depth + 1);
    return String(value);
  };
  return template.replace(/\{(\w+)\}/g, (whole, key) => (args && Object.hasOwn(args, key) ? argument(args[key]) : whole));
}

/** A failure object (`ok: false` with `error` and `messageId`) with its text in the language of `locale`; the objects inside an answer too (the package plan's `plan.dir`). */
function localizeObject(object, locale, depth) {
  if (object.ok === false && typeof object.error === 'string' && typeof object.messageId === 'string') {
    object.error = render(locale.english, object.error, object.messageId, object.messageArgs);
  }
  if (depth < 3) {
    for (const value of Object.values(object)) {
      if (value && typeof value === 'object' && !Array.isArray(value)) localizeObject(value, locale, depth + 1);
    }
  }
}

/** The answer of a request with its failure texts in the language of `locale` ({ language, english }). */
function localize(answer, locale) {
  const header = answer && answer.header;
  if (!header || typeof header !== 'object' || !locale.english || locale.language === 'ko') return answer;
  localizeObject(header, locale, 0);
  return answer;
}

module.exports = { render, localize };
