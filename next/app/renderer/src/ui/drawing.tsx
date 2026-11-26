/** Drawing gestures. The resulting page coordinates are tested by basic-create.
 * Shift/Alt follow Adobe's documented drawing shortcuts; hidden DOM scripts cannot
 * synthesize native mouse gestures. The native paths/defaults are measured separately. */
import type { DrawKind } from '../engine';
import type { Box } from '../inspect';
import type { ToolId } from './tools';
import { useDrawingSettings } from './drawing-settings';
import { ownPoint, ownRect } from './turned';
import { shiftLine } from './dragunit';

export const DRAW_TOOLS: Partial<Record<ToolId, DrawKind>> = {
  rect: 'rectangle', ellipse: 'ellipse', polygon: 'polygon', line: 'line',
  rectFrame: 'rectangleFrame', ellipseFrame: 'ellipseFrame', polygonFrame: 'polygonFrame',
  type: 'textFrame', typeVertical: 'textFrame',
};
/** `cells`: the grid the arrow keys asked for while drawing (ui/drawgrid.ts); the preview draws each cell. */
export type Drawing = { page: number; kind: DrawKind; rect: Box; cells?: Box[] };

/**
 * The box a drag draws, in the page as shown. Shift makes the largest shape of its own proportions that fits in the drag (measured in InDesign 2025 with a
 * real mouse, liveinput-c2: a 150 x 60 drag gives 60 x 60, a hexagon 69.28 x 60, in every turn of the spread); Alt draws about the point pressed
 * (the corner dragged from is then the middle) and with Shift the smaller of the two doubled sides is the side. `aspect`: width over height of the
 * polygon's own box (the engine's `drawingSettings`), 1 for the other shapes.
 */
export function drawRect(kind: DrawKind, start: [number, number], end: [number, number], shift: boolean, alt: boolean, aspect = 1): Box {
  const [x, y] = start;
  let dx = end[0] - x, dy = end[1] - y;
  if (shift) {
    if (kind === 'line') {
      // the angle to a multiple of 45 degrees, the shorter way on a diagonal (measured on the rounded points: draground-c1)
      [dx, dy] = shiftLine([dx, dy]);
    } else {
      const ratio = kind.startsWith('polygon') && aspect > 0 ? aspect : 1;
      const height = Math.min(Math.abs(dx) / ratio, Math.abs(dy));
      dx = (dx < 0 ? -1 : 1) * height * ratio; dy = (dy < 0 ? -1 : 1) * height;
    }
  }
  const r: Box = [alt ? x - dx : x, alt ? y - dy : y, x + dx, y + dy];
  return kind === 'line' ? r : [Math.min(r[0], r[2]), Math.min(r[1], r[3]), Math.max(r[0], r[2]), Math.max(r[1], r[3])];
}

/** `turn`: the quarter turn of the page's spread; the drag is read as the page is shown, the preview is drawn in the page's own space. */
export function DrawingPreview({ drawing: d, width, height, turn = 0 }: { drawing: Drawing; width: number; height: number; turn?: number }) {
  const settings = useDrawingSettings();
  const draw = (box: Box, key?: number) => {
    const [x, y, r, b] = ownRect(turn, width, height, box, d.kind === 'line'), w = r - x, h = b - y;
    return d.kind === 'line' ? <line key={key} x1={x} y1={y} x2={r} y2={b} />
      : d.kind.startsWith('ellipse') ? <ellipse key={key} cx={x + w / 2} cy={y + h / 2} rx={w / 2} ry={h / 2} />
      // the polygon stands upright in the view whatever the turn (the mouse stores it so): its points are made in the box as shown, then put in the page's own space
      : d.kind.startsWith('polygon') ? <polygon key={key} points={settings?.points.map(([px, py]) => { const [qx, qy] = ownPoint(turn, width, height, [box[0] + (box[2] - box[0]) * px, box[1] + (box[3] - box[1]) * py]); return `${qx},${qy}`; }).join(' ')} />
      : <rect key={key} x={x} y={y} width={w} height={h} />;
  };
  return <svg className="drawing-preview" data-cells={d.cells?.length ?? 1} viewBox={`0 0 ${width} ${height}`} style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', overflow: 'visible', pointerEvents: 'none', zIndex: 4 }}>
    <g fill="none" stroke="#1676ef" strokeWidth={1} style={{ vectorEffect: 'non-scaling-stroke' }}>{d.cells ? d.cells.map((c, i) => draw(c, i)) : draw(d.rect)}</g>
  </svg>;
}
