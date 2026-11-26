export type CaptionSetup = {
  rows: { before: string; metadata: string; after: string }[];
  position: 'below' | 'above' | 'left' | 'right';
  offset: number; paragraphStyle: string; layer: string; group: boolean;
};
