//! Shared input and document state for the path tools.

import type { EditCommand, EditResult } from '../engine';
import type { DocumentInfo, ItemInfo, ItemStore } from '../inspect';
import type { LayerLook, SelEntry } from './overlay';
import type { PencilPrefs } from './pencilprefs';
import type { PointSnap, SkipPoints } from './pointsnap';
import type { SmartDraw } from './smartguides';
import type { ToolId } from './tools';

export type Here = { index: number; el: HTMLElement; x: number; y: number };
export type Hover = { page: number; x: number; y: number; clientX: number; clientY: number };
export type PageId = (index: number) => string | undefined;
export type ShowSnap = (page: number, snap: PointSnap | null) => void;
export type SmartAt = (page: number, point: [number, number], skip: SkipPoints, anchorsOnly?: boolean) => [number, number] | null;

export type Options = {
  onKey?: (event: KeyboardEvent) => boolean;
  tool: ToolId;
  editable: boolean;
  info: DocumentInfo | null;
  store: ItemStore;
  layers: LayerLook;
  selection: SelEntry[];
  scale: number;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
  setSelection: (s: SelEntry[]) => void;
  note: (text: string) => void;
  /** A point of a page pulled onto guides, page edges and gridlines near it (Snap to Guides / Document Grid); absent: no snapping. */
  snap?: (page: number, pt: [number, number]) => [number, number];
  /** A pointer position put on InDesign's drag grid for the zoom (draground-c1: a dragged anchor point lands on the grid point of the pointer); absent: not rounded. */
  grid?: (page: number, pt: [number, number]) => [number, number];
  /** The shape under a client point (the Add / Delete Anchor Point and Convert Direction Point tools work on any shape). */
  hit: (clientX: number, clientY: number) => { page: number; item: ItemInfo } | null;
  /** The page place under a client point; the Pencil takes every pointer position the browser merged into one event with it. */
  pageAt?: (clientX: number, clientY: number) => { index: number; x: number; y: number } | null;
  /** The Pencil Tool Preferences (a double click on the Pencil tool); absent: the installed values. */
  pencil?: PencilPrefs;
  /** The quarter turn (View ▸ Rotate Spread) of a page's spread: the arrow keys move the points the way they point on the screen. */
  turn?: (page: number) => number | undefined;
  /** Smart Guides for one point (ui/pointsnap.ts): the anchors of the paths, the edges and middles of the objects; null while they are off.
   *  `skip`: the points being moved. */
  smartPoint?: (page: number, pt: [number, number], skip: SkipPoints) => PointSnap | null;
  /** Shows the Smart Guides lines of a point (null: none). */
  showSmart?: (page: number, draw: SmartDraw | null) => void;
};
