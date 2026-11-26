// View ▸ Extras ▸ Show Notes: InDesign marks each editorial note's anchor in the text with an
// orange hourglass (two triangles meeting at the middle), centred on the anchor, half the type
// size wide, from 0.7 of the size above the baseline to 0.1 below it. It takes no room in the
// text, shows in Normal view only and never prints. Measured in InDesign 2025 (colour
// 255,153,0 whatever the user or the note colour preference; artifacts/notes-view-20261025).
// The engine (`textMarks`, only: notes) gives each anchor on its baseline and, above it, one
// type size up, so a turned frame turns the marker.
import { useEffect, useState } from 'react';

type Mark = { kind: string; frame: string; from: [number, number]; up: [number, number] };

type Props = { page: number; width: number; height: number; rev: number };

export function NoteMarks({ page, width, height, rev }: Props) {
  const [marks, setMarks] = useState<Mark[]>([]);
  useEffect(() => {
    let live = true;
    window.folio.request({ cmd: 'textMarks', page, only: 'notes' }).then(({ header }) => {
      if (live && header.ok) setMarks((header.marks as Mark[]) ?? []);
    }, () => { /* no document */ });
    return () => { live = false; };
  }, [page, rev]);
  if (!marks.length) return null;
  // multiply: the marker sits behind the text in InDesign, so black glyphs stay black over it
  return (
    <svg className="note-marks" viewBox={`0 0 ${width} ${height}`} aria-hidden="true"
      style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none', overflow: 'visible', mixBlendMode: 'multiply' }}>
      {marks.map((m, i) => {
        const ux = m.up[0] - m.from[0], uy = m.up[1] - m.from[1];
        const size = Math.hypot(ux, uy) || 12;
        const angle = Math.atan2(ux, -uy) * 180 / Math.PI;
        const half = size / 4, top = 0.7 * size, bottom = 0.1 * size, mid = (bottom - top) / 2;
        return (
          <path key={i} className="note-mark" fill="rgb(255,153,0)" transform={`translate(${m.from[0]} ${m.from[1]}) rotate(${angle})`}
            d={`M${-half} ${-top}H${half}L0 ${mid}ZM${-half} ${bottom}H${half}L0 ${mid}Z`} />
        );
      })}
    </svg>
  );
}
