export type TocEntry = {
  name: string; formatStyle: string; level: number; pageNumberPosition: string;
  pageNumberStyle: string; separator: string; separatorStyle: string; sortAlphabet: boolean;
};
export type TocStyle = {
  id: string; name: string; title: string; titleStyle: string; entries: TocEntry[];
  includeHidden: boolean; includeBookDocuments: boolean; createBookmarks: boolean;
  makeAnchor: boolean; runIn: boolean; removeForcedLineBreak: boolean; numberedParagraphs: string;
};
export type TocSource = { story: string; paragraph: number; page: string; pageIndex: number; hidden: boolean; number: string; book?: { document: string; style: string; text: string } };
export type TextLocation = { source: TocSource; start: number; end: number; paragraphStart: boolean };
