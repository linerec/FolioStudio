"""Separations with Ghostscript tiffsep and plate-by-plate comparison.

  python sep.py render <file.pdf> <outdir> [dpi]      # gs tiffsep, one TIFF per page and plate
  python sep.py compare <refdir> <canddir> <out.json> [pages-for-pictures...]

Plate files are named p001(Cyan).tif; values are ink coverage inverted (255 = no ink).
Error metric: mean absolute difference of ink coverage in percent (0-100) per page and plate,
over pixels of the page; a plate missing on one side counts as zero ink there.
"""
import sys, os, re, json, subprocess, pathlib, time, collections, shutil
sys.stdout.reconfigure(encoding='utf-8')
import numpy as np
from PIL import Image

def ghostscript():
    exe = os.environ.get('FOLIO_GHOSTSCRIPT') or shutil.which('gswin64c') or shutil.which('gs')
    if not exe:
        raise RuntimeError('Ghostscript not found: set FOLIO_GHOSTSCRIPT or PATH')
    return exe


def render(pdf, outdir, dpi=150):
    out = pathlib.Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    for f in out.glob('p*.tif'):
        f.unlink()
    t = time.time()
    cmd = [ghostscript(), '-q', '-dSAFER', '-dBATCH', '-dNOPAUSE', '-sDEVICE=tiffsep', f'-r{dpi}', '-dUsePDFX3Profile',
           '-sCompression=lzw', f'-sOutputFile={out / "p%03d.tif"}', str(pdf)]
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    meta = {'pdf': str(pdf), 'dpi': dpi, 'seconds': time.time() - t, 'returncode': r.returncode,
            'stderr': r.stderr[-4000:], 'cmd': cmd}
    (out / 'render.json').write_text(json.dumps(meta, indent=1), encoding='utf-8')
    print(json.dumps({k: meta[k] for k in ('seconds', 'returncode')}), r.stderr[-500:])


def plates(d):
    by_page = collections.defaultdict(dict)
    for f in pathlib.Path(d).glob('p*(*).tif'):
        m = re.match(r'p(\d+)\((.*)\)\.tif$', f.name)
        if m:
            by_page[int(m.group(1)) - 1][m.group(2)] = f
    return by_page


def coverage(f):
    a = np.asarray(Image.open(f), dtype=np.float32)
    if a.ndim == 3:
        a = a[..., 0]
    return (255.0 - a) * (100.0 / 255.0)


def compare(ref, cand, out, pictures=()):
    A, B = plates(ref), plates(cand)
    pages = sorted(set(A) | set(B))
    names_a = sorted({n for p in A.values() for n in p})
    names_b = sorted({n for p in B.values() for n in p})
    rows = []
    per_plate = collections.defaultdict(list)
    for p in pages:
        pa, pb = A.get(p, {}), B.get(p, {})
        shape = None
        for f in list(pa.values()) + list(pb.values()):
            shape = Image.open(f).size[::-1]
            break
        row = {'page': p, 'plates': {}, 'onlyRef': sorted(set(pa) - set(pb)), 'onlyCand': sorted(set(pb) - set(pa))}
        for n in sorted(set(pa) | set(pb)):
            ca = coverage(pa[n]) if n in pa else None
            cb = coverage(pb[n]) if n in pb else None
            if ca is None:
                ca = np.zeros_like(cb)
            if cb is None:
                cb = np.zeros_like(ca)
            h, w = min(ca.shape[0], cb.shape[0]), min(ca.shape[1], cb.shape[1])
            d = np.abs(ca[:h, :w] - cb[:h, :w])
            e = {'mae': float(d.mean()), 'inkRef': float(ca.mean()), 'inkCand': float(cb.mean()),
                 'diff10': float((d > 10).mean() * 100), 'sizeRef': list(ca.shape), 'sizeCand': list(cb.shape)}
            row['plates'][n] = e
            per_plate[n].append(e['mae'])
            if p in pictures:
                pic = np.concatenate([255 - ca[:h, :w] * 2.55, np.full((h, 6), 128.0), 255 - cb[:h, :w] * 2.55,
                                      np.full((h, 6), 128.0), 255 - np.minimum(d * 2.55 * 4, 255)], axis=1)
                Image.fromarray(pic.clip(0, 255).astype(np.uint8)).save(pathlib.Path(out).parent / f'{pathlib.Path(out).stem}-p{p:03d}-{n}.png')
        rows.append(row)
    worst = sorted(((e['mae'], r['page'], n) for r in rows for n, e in r['plates'].items()), reverse=True)[:15]
    summary = {
        'pages': len(pages), 'pagesRef': len(A), 'pagesCand': len(B),
        'platesRef': names_a, 'platesCand': names_b, 'plateSetsEqual': names_a == names_b,
        'pagesWithDifferentPlateSets': [r['page'] for r in rows if r['onlyRef'] or r['onlyCand']],
        'perPlateMeanMAE': {n: float(np.mean(v)) for n, v in per_plate.items()},
        'perPlateMedianMAE': {n: float(np.median(v)) for n, v in per_plate.items()},
        'allPlatesMeanMAE': float(np.mean([e['mae'] for r in rows for e in r['plates'].values()])),
        'worst': [{'mae': m, 'page': p, 'plate': n} for m, p, n in worst],
    }
    pathlib.Path(out).write_text(json.dumps({'summary': summary, 'pages': rows}, indent=1), encoding='utf-8')
    print(json.dumps(summary, ensure_ascii=False)[:3000])


if __name__ == '__main__':
    if sys.argv[1] == 'render':
        render(sys.argv[2], sys.argv[3], int(sys.argv[4]) if len(sys.argv) > 4 else 150)
    elif sys.argv[1] == 'compare':
        compare(sys.argv[2], sys.argv[3], sys.argv[4], {int(x) for x in sys.argv[5:]})
