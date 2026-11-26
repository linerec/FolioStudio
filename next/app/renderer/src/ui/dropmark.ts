import type React from 'react';

/**
 * Where a drag over a layer row would land, shown on the row itself (uix-b4: the same two marks as the panel dock, ui/motion.css):
 * a layer dragged over the upper or lower half goes above or below it ('before' / 'after': a line), objects dragged onto it go into
 * it ('into': a ring). The drop itself decides the same way (ui/panels.tsx). Only the row's own attribute changes: no React state,
 * so a drag does not re-render the panel at every pointer move. While dragging, the data cannot be read, only its types.
 */
export function markLayerDrop(e: React.DragEvent<HTMLElement>) {
  const row = e.currentTarget, types = e.dataTransfer.types;
  const r = row.getBoundingClientRect();
  const at = types.includes('folio/layer') ? (e.clientY < r.top + r.height / 2 ? 'before' : 'after') : types.includes('folio/items') ? 'into' : '';
  if (row.dataset.drop !== at) { if (at) row.dataset.drop = at; else delete row.dataset.drop; }
}

/** The drag left the row (not just into one of its own parts), or was let go or cancelled over it. */
export function clearLayerDrop(e: React.DragEvent<HTMLElement>) {
  const into = e.relatedTarget;
  if (e.type === 'dragleave' && into instanceof Node && e.currentTarget.contains(into)) return;
  delete e.currentTarget.dataset.drop;
}
