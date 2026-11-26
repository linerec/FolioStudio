"""Where the bytes of a PDF go: stream sizes by kind (image filter/colour space, form, font,
content, ICC). Usage: python pdfsize.py file.pdf [top N largest]"""
import sys, collections
sys.stdout.reconfigure(encoding='utf-8')
from pypdf import PdfReader
from pypdf.generic import IndirectObject, StreamObject

r = PdfReader(sys.argv[1])
top = int(sys.argv[2]) if len(sys.argv) > 2 else 10
kinds = collections.Counter()
counts = collections.Counter()
largest = []
for num in range(1, r.trailer['/Size']):
    try:
        o = r.get_object(IndirectObject(num, 0, r))
    except Exception:
        continue
    if not isinstance(o, StreamObject):
        continue
    n = len(o._data) if hasattr(o, '_data') else 0
    st = o.get('/Subtype')
    if st == '/Image':
        cs = o.get('/ColorSpace')
        cs = cs.get_object() if isinstance(cs, IndirectObject) else cs
        csn = str(cs[0]) if isinstance(cs, list) else str(cs)
        k = f"image {o.get('/Filter')} {csn} {'smask' if o.get('/ColorSpace') == '/DeviceGray' and o.get('/SMask') is None else ''}"
        largest.append((n, num, f"{o.get('/Width')}x{o.get('/Height')} {o.get('/Filter')} {csn}"))
    elif st == '/Form':
        k = 'form'
        largest.append((n, num, 'form'))
    elif o.get('/Type') == '/Metadata':
        k = 'xmp'
    elif '/N' in o and '/Length1' not in o and o.get('/Type') is None and o.get('/Subtype') is None and o.get('/FunctionType') is None:
        k = 'icc?'
    elif '/Length1' in o or o.get('/Subtype') in ('/CIDFontType0C', '/Type1C', '/OpenType'):
        k = 'font'
    else:
        k = 'other (content, cmap, ...)'
    kinds[k] += n
    counts[k] += 1
total = sum(kinds.values())
print(f'streams total {total/1e6:.2f} MB of file')
for k, v in kinds.most_common():
    print(f'  {v/1e6:9.2f} MB  {counts[k]:5d}  {k}')
for n, num, d in sorted(largest, reverse=True)[:top]:
    print(f'  obj {num}: {n/1e6:.2f} MB {d}')
