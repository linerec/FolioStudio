export const autoCorrectLanguages: string[] = [
  "Arabic",
  "Bulgarian",
  "Catalan",
  "Croatian",
  "Czech",
  "Danish",
  "Dutch: Old Rules",
  "English: Canadian",
  "English: UK",
  "English: USA",
  "Estonian",
  "Finnish",
  "French",
  "French: Canadian",
  "German: 1996 Reform",
  "German: Austria 2006 Reform",
  "German: Swiss",
  "Greek",
  "Hebrew",
  "Hungarian",
  "Italian",
  "Latvian",
  "Lithuanian",
  "Norwegian: Bokmål",
  "Norwegian: Nynorsk",
  "Polish",
  "Portuguese",
  "Portuguese: Brazilian",
  "Romanian",
  "Russian",
  "Slovak",
  "Slovenian",
  "Spanish",
  "Swedish",
  "Thai",
  "Turkish",
  "Ukrainian",
  "Vietnamese"
];
export type AutoCorrectTables = Record<string, [string, string][]>;
export function cleanAutoCorrectTables(value: unknown): AutoCorrectTables {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  const tables: AutoCorrectTables = {};
  for (const [language, rows] of Object.entries(value)) {
    if (!autoCorrectLanguages.includes(language) || !Array.isArray(rows)) continue;
    const seen = new Set<string>();
    tables[language] = rows.filter((pair): pair is [string, string] => {
      if (!Array.isArray(pair) || pair.length !== 2 || pair.some(word => typeof word !== 'string') || seen.has(pair[0])) return false;
      seen.add(pair[0]); return true;
    }).map(([word, correction]) => [word, correction]);
  }
  return tables;
}
