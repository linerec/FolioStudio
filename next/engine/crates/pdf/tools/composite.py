"""Composite comparison of two PDFs with the metric of next/app/scripts/fidelity.py:
PyMuPDF raster at 72 dpi, RGB, mean absolute channel difference /255, pixels whose largest
channel difference exceeds 16. Usage: composite.py ref.pdf cand.pdf out.json [side-by-side pages...]"""
import json, sys, pathlib
sys.stdout.reconfigure(encoding='utf-8')
import fitz, numpy as np
from PIL import Image

ref_pdf, cand_pdf, out = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
keep = {int(x) for x in sys.argv[4:]}
a, b = fitz.open(ref_pdf), fitz.open(cand_pdf)
rows = []
def raster(page):
    pm = page.get_pixmap(matrix=fitz.Matrix(1, 1), colorspace=fitz.csRGB, alpha=False)
    return np.frombuffer(pm.samples, np.uint8).reshape(pm.height, pm.width, 3).astype(np.int16)
for i in range(min(len(a), len(b))):
    ref, cand = raster(a[i]), raster(b[i])
    h, w = max(ref.shape[0], cand.shape[0]), max(ref.shape[1], cand.shape[1])
    pad = lambda x: np.pad(x, ((0, h - x.shape[0]), (0, w - x.shape[1]), (0, 0)), constant_values=255)
    d = np.abs(pad(ref) - pad(cand))
    rows.append({'page': i, 'mae255': float(d.mean()), 'changed': int((d.max(axis=2) > 16).sum()), 'pixels': h * w, 'sameSize': ref.shape == cand.shape})
    if i in keep:
        side = np.concatenate([pad(ref), np.full((h, 8, 3), 128, np.int16), pad(cand)], axis=1).clip(0, 255).astype(np.uint8)
        Image.fromarray(side).save(out.parent / f'{out.stem}-side-{i:03d}.png')
mae = [r['mae255'] for r in rows]
summary = {'pages': len(rows), 'pagesRef': len(a), 'pagesCand': len(b), 'meanMAE255': float(np.mean(mae)), 'medianMAE255': float(np.median(mae)),
           'changedPct': 100.0 * sum(r['changed'] for r in rows) / max(1, sum(r['pixels'] for r in rows)),
           'worst': sorted(rows, key=lambda r: -r['mae255'])[:8], 'sizeMismatch': sum(not r['sameSize'] for r in rows)}
out.write_text(json.dumps({'summary': summary, 'pages': rows}, indent=1), encoding='utf-8')
print(json.dumps({k: v for k, v in summary.items() if k != 'worst'}), [(r['page'], round(r['mae255'], 2)) for r in summary['worst']])
