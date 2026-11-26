"""Inventory of a PDF's print-relevant structure: boxes, page groups, colour spaces, fonts,
images, ExtGStates (overprint / blend), shadings. Usage: python pdfinv.py file.pdf [out.json]"""
import sys, json, collections
sys.stdout.reconfigure(encoding='utf-8')
from pypdf import PdfReader
from pypdf.generic import IndirectObject, ArrayObject, DictionaryObject, NameObject

def res(o):
    return o.get_object() if isinstance(o, IndirectObject) else o

def cs_desc(cs):
    cs = res(cs)
    if isinstance(cs, NameObject):
        return str(cs)
    if isinstance(cs, ArrayObject):
        kind = str(cs[0])
        if kind == '/Separation':
            return f'Separation {cs[1]} alt={cs_desc(cs[2])}'
        if kind == '/DeviceN':
            names = [str(n) for n in res(cs[1])]
            return f'DeviceN {names} alt={cs_desc(cs[2])}'
        if kind == '/ICCBased':
            s = res(cs[1]); return f'ICCBased N={s.get("/N")}'
        if kind == '/Indexed':
            return f'Indexed base={cs_desc(cs[1])}'
        if kind == '/Lab':
            return 'Lab'
        return kind
    return str(type(cs))

def main(path):
    r = PdfReader(path)
    inv = {'pages': len(r.pages), 'colorspaces': collections.Counter(), 'fonts': collections.Counter(),
           'images': collections.Counter(), 'extgstate': collections.Counter(), 'shadings': collections.Counter(),
           'boxes': collections.Counter(), 'groups': collections.Counter(), 'xobjects': collections.Counter(),
           'inks': collections.Counter()}
    seen = set()
    def walk_res(rd):
        rd = res(rd)
        if not isinstance(rd, DictionaryObject):
            return
        for k, v in (res(rd.get('/ColorSpace')) or {}).items():
            d = cs_desc(v); inv['colorspaces'][d] += 1
            if d.startswith('Separation'): inv['inks'][d.split()[1]] += 1
            if d.startswith('DeviceN'):
                for n in eval(d.split(' alt=')[0][8:]): inv['inks'][n] += 1
        for k, v in (res(rd.get('/Font')) or {}).items():
            key = v.idnum if isinstance(v, IndirectObject) else id(v)
            if ('f', key) in seen: continue
            seen.add(('f', key)); f = res(v)
            desc = str(f.get('/Subtype'))
            if f.get('/Subtype') == '/Type0':
                d = res(res(f['/DescendantFonts'])[0]); fd = res(d.get('/FontDescriptor'))
                ff = [x for x in ('/FontFile', '/FontFile2', '/FontFile3') if x in fd]
                sub = res(fd[ff[0]]).get('/Subtype') if ff else None
                desc = f"Type0/{d.get('/Subtype')} enc={f.get('/Encoding')} file={ff} {sub} tounicode={'/ToUnicode' in f}"
            else:
                fd = res(f.get('/FontDescriptor')) or {}
                ff = [x for x in ('/FontFile', '/FontFile2', '/FontFile3') if x in fd]
                desc += f' file={ff} tounicode={"/ToUnicode" in f}'
            inv['fonts'][desc] += 1
        for k, v in (res(rd.get('/ExtGState')) or {}).items():
            g = res(v)
            desc = ' '.join(f'{kk}={g[kk]}' for kk in ('/OP', '/op', '/OPM', '/BM', '/CA', '/ca', '/SMask', '/AIS', '/TK') if kk in g and not (kk=='/SMask' and g[kk]=='/None'))
            inv['extgstate'][desc] += 1
        for k, v in (res(rd.get('/Shading')) or {}).items():
            s = res(v); inv['shadings'][f"type={s.get('/ShadingType')} cs={cs_desc(s.get('/ColorSpace'))}"] += 1
        for k, v in (res(rd.get('/XObject')) or {}).items():
            key = v.idnum if isinstance(v, IndirectObject) else id(v)
            if ('x', key) in seen: continue
            seen.add(('x', key)); x = res(v)
            if x.get('/Subtype') == '/Image':
                inv['images'][f"cs={cs_desc(x.get('/ColorSpace')) if '/ColorSpace' in x else 'mask'} bpc={x.get('/BitsPerComponent')} filter={x.get('/Filter')} smask={'/SMask' in x} decode={x.get('/Decode')}"] += 1
                if '/SMask' in x:
                    sm = res(x['/SMask']); inv['images'][f"  smask cs={cs_desc(sm.get('/ColorSpace'))} filter={sm.get('/Filter')}"] += 1
            else:
                g = res(x.get('/Group'))
                gd = f" group(S={g.get('/S')} CS={cs_desc(g.get('/CS')) if g and '/CS' in g else None} I={g.get('/I')} K={g.get('/K')})" if g else ''
                inv['xobjects'][f"Form{gd}"] += 1
                walk_res(x.get('/Resources'))
    for i, p in enumerate(r.pages):
        b = {k: [round(float(v), 2) for v in p[k]] for k in ('/MediaBox', '/CropBox', '/BleedBox', '/TrimBox', '/ArtBox') if k in p}
        mb = b.get('/MediaBox'); tb = b.get('/TrimBox')
        inv['boxes'][json.dumps({k: [round(v - mb[j % 2], 2) for j, v in enumerate(val)] for k, val in b.items()})] += 1
        g = res(p.get('/Group'))
        inv['groups'][f"S={g.get('/S')} CS={cs_desc(g.get('/CS'))} I={g.get('/I')} K={g.get('/K')}" if g else 'none'] += 1
        walk_res(p.get('/Resources'))
    out = {k: (dict(v.most_common()) if isinstance(v, collections.Counter) else v) for k, v in inv.items()}
    return out

if __name__ == '__main__':
    o = main(sys.argv[1])
    text = json.dumps(o, indent=1, ensure_ascii=False)
    if len(sys.argv) > 2:
        open(sys.argv[2], 'w', encoding='utf-8').write(text)
    print(text[:int(sys.argv[3]) if len(sys.argv) > 3 else 6000])
