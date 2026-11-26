"""PDF/X-1a:2001 (ISO 15930-1:2001) and PDF/X-3:2002 (ISO 15930-3:2002) structure check.

Usage: python pdfx1acheck.py file.pdf [--x3] [out.json]

Both are PDF 1.3: header version 1.3 or lower; no object or cross-reference streams; no
encryption; trailer ID; Info Title, CreationDate, ModDate, Trapped True/False,
GTS_PDFXVersion (PDF/X-1:2001 or PDF/X-3:2002) and GTS_PDFXConformance; one GTS_PDFX output
intent with OutputConditionIdentifier and, embedded, a valid output-class CMYK profile
(DestOutputProfile, ICC version 2 or 4 header; required unless the identifier is registered);
XMP identification consistent with Info when present; page boxes (TrimBox or ArtBox, nested
MediaBox ⊇ BleedBox ⊇ TrimBox); every font embedded; no transparency anywhere (soft masks,
constant alpha below 1, blend modes other than Normal/Compatible, transparency groups, image
soft masks); no PDF 1.4+ features (JBIG2, JPX, 16-bit images, optional content); no transfer
functions, OPI, PostScript XObjects or operators, image Alternates, Interpolate true; no
JavaScript, embedded files or forbidden actions; annotations only PrinterMark / TrapNet inside
the bleed box; PDF 1.3 implementation limits (q nesting 28, DeviceN 8 colorants, arrays 8191,
dictionaries 4095, names 127 bytes, strings 65535 bytes, integers 2³¹−1).
Colour, PDF/X-1a: only DeviceCMYK, DeviceGray, Separation / DeviceN with a DeviceCMYK or
DeviceGray alternate, Indexed over those and Pattern; nothing RGB, Lab, CalRGB, CalGray or
ICC-based (resources, images, shadings, page groups, inline images, rg/RG operators).
Colour, PDF/X-3: DeviceRGB only where DefaultRGB is set; ICC-based profiles valid, N matching,
ICC version 2 (what PDF 1.3 holds).
Every object in the file is scanned for the transparency and PDF 1.4+ keys, referenced or not.
This is a structural check, not a certified validator.
"""
import sys, json, re, collections
sys.stdout.reconfigure(encoding='utf-8')
from pypdf import PdfReader
from pypdf.generic import IndirectObject, ArrayObject, DictionaryObject, NameObject, StreamObject, NumberObject, FloatObject, TextStringObject, ByteStringObject


def res(o):
    return o.get_object() if isinstance(o, IndirectObject) else o


class Report:
    def __init__(self):
        self.items = []
        self.counts = collections.Counter()

    def check(self, name, ok, detail='', level='fail'):
        status = 'pass' if ok else level
        self.items.append({'check': name, 'status': status, 'detail': detail})
        self.counts[status] += 1


def icc_header(data):
    if len(data) < 128 or data[36:40] != b'acsp':
        return None
    return {'size': int.from_bytes(data[0:4], 'big'), 'major': data[8], 'version': f'{data[8]}.{data[9] >> 4}', 'class': data[12:16].decode('latin-1'),
            'space': data[16:20].decode('latin-1').strip(), 'pcs': data[20:24].decode('latin-1').strip()}


REGISTERED = {'CGATS TR 001', 'CGATS TR 003', 'CGATS TR 005', 'CGATS TR 006', 'FOGRA27', 'FOGRA28', 'FOGRA29', 'FOGRA39', 'FOGRA51', 'FOGRA52', 'JC200103'}


def shorten(v, n=5):
    return '; '.join(v[:n]) + (f' (+{len(v) - n})' if len(v) > n else '')


def main(path, x3):
    rep = Report()
    raw = open(path, 'rb').read()
    m = re.match(rb'%PDF-(\d)\.(\d)', raw[:16])
    version = (int(m.group(1)), int(m.group(2))) if m else None
    rep.check('header PDF version <= 1.3', version is not None and version <= (1, 3), f'header {raw[:8]!r}')
    rep.check('no object streams or cross-reference streams (PDF 1.5)', b'/ObjStm' not in raw and b'/XRefStm' not in raw and not re.search(rb'/Type\s*/XRef\b', raw))
    r = PdfReader(path)
    t = r.trailer
    rep.check('not encrypted', '/Encrypt' not in t)
    ids = t.get('/ID')
    rep.check('trailer /ID present (two strings)', isinstance(ids, ArrayObject) and len(ids) == 2)
    cat = res(t['/Root'])
    want_version = 'PDF/X-3:2002' if x3 else 'PDF/X-1:2001'
    want_conf = 'PDF/X-3:2002' if x3 else 'PDF/X-1a:2001'

    # Info dictionary.
    info = res(t.get('/Info')) or {}
    rep.check(f'Info GTS_PDFXVersion = {want_version}', str(info.get('/GTS_PDFXVersion')) == want_version, str(info.get('/GTS_PDFXVersion')))
    conf = info.get('/GTS_PDFXConformance')
    rep.check(f'Info GTS_PDFXConformance = {want_conf}', str(conf) == want_conf if not x3 else conf is None or str(conf) == want_conf, str(conf))
    for k in ('/Title', '/CreationDate', '/ModDate'):
        rep.check(f'Info {k[1:]} present', bool(info.get(k)), str(info.get(k))[:60])
    rep.check('Info Trapped /True or /False', str(info.get('/Trapped')) in ('/True', '/False'), str(info.get('/Trapped')))

    # Output intent.
    ois = res(cat.get('/OutputIntents')) or []
    gts = [res(o) for o in ois if res(o).get('/S') == '/GTS_PDFX']
    rep.check('exactly one GTS_PDFX output intent', len(gts) == 1, f'{len(gts)} found')
    if gts:
        oi = gts[0]
        ident = str(oi.get('/OutputConditionIdentifier') or '')
        rep.check('OutputConditionIdentifier present', bool(ident), ident)
        prof = res(oi.get('/DestOutputProfile'))
        rep.check('DestOutputProfile embedded (or a registered identifier)', isinstance(prof, StreamObject) or ident in REGISTERED, ident)
        if isinstance(prof, StreamObject):
            h = icc_header(prof.get_data())
            rep.check('output profile is a valid ICC profile', h is not None, json.dumps(h))
            if h:
                rep.check('output profile is CMYK (or gray) with matching N', h['space'].upper() in ('CMYK', 'GRAY') and {'CMYK': 4, 'GRAY': 1}.get(h['space'].upper()) == prof.get('/N'), f"N={prof.get('/N')} space={h['space']}")
                rep.check('output profile class is output (prtr)', h['class'] == 'prtr', h['class'])
        rep.check('RegistryName present for a registered identifier', bool(oi.get('/RegistryName')) or ident not in REGISTERED, str(oi.get('/RegistryName')), level='warn')

    # XMP, when present.
    md = res(cat.get('/Metadata'))
    if isinstance(md, StreamObject):
        xmp = md.get_data().decode('utf-8', 'replace')
        def xget(tag):
            mm = re.search(r'<' + tag + r'>(.*?)</' + tag + r'>', xmp, re.S) or re.search(tag + r'="([^"]*)"', xmp)
            return re.sub(r'<[^>]+>', '', mm.group(1)).strip() if mm else None
        rep.check('XMP GTS_PDFXVersion agrees with Info (when present)', xget('pdfx:GTS_PDFXVersion') in (None, want_version) and xget('pdfxid:GTS_PDFXVersion') in (None, want_version), f"{xget('pdfxid:GTS_PDFXVersion')} / {xget('pdfx:GTS_PDFXVersion')}")
        rep.check('XMP GTS_PDFXConformance agrees (when present)', xget('pdfx:GTS_PDFXConformance') in (None, want_conf), str(xget('pdfx:GTS_PDFXConformance')))
        rep.check('XMP Trapped agrees with Info', xget('pdf:Trapped') in (None, str(info.get('/Trapped')).lstrip('/')), str(xget('pdf:Trapped')))

    names = res(cat.get('/Names')) or {}
    rep.check('no JavaScript name tree', '/JavaScript' not in names)
    rep.check('no embedded files', '/EmbeddedFiles' not in names)
    rep.check('no document OpenAction JavaScript / catalog additional actions', not (cat.get('/OpenAction') and 'JavaScript' in str(res(cat.get('/OpenAction')))) and '/AA' not in cat)
    rep.check('no optional content (PDF 1.5)', '/OCProperties' not in cat)

    # Every object: transparency, PDF 1.4+ features, implementation limits.
    transparency, newer, limits, forbidden = [], [], [], collections.Counter()
    def scan_value(v, where, depth=0):
        if depth > 64:
            return
        if isinstance(v, ArrayObject):
            if len(v) > 8191:
                limits.append(f'{where}: array of {len(v)}')
            for x in v:
                scan_value(x, where, depth + 1)
        elif isinstance(v, DictionaryObject):
            if len(v) > 4095:
                limits.append(f'{where}: dictionary of {len(v)}')
            for k, x in v.items():
                if len(k) > 128:
                    limits.append(f'{where}: name {k[:20]}…')
                scan_value(x, where, depth + 1)
        elif isinstance(v, NameObject):
            if len(v) > 128:
                limits.append(f'{where}: name of {len(v)} bytes')
        elif isinstance(v, (TextStringObject, ByteStringObject)):
            if len(bytes(v.original_bytes if hasattr(v, 'original_bytes') else v)) > 65535:
                limits.append(f'{where}: string over 65535 bytes')
        elif isinstance(v, NumberObject):
            if abs(int(v)) > 2147483647:
                limits.append(f'{where}: integer {int(v)}')
    for num in range(1, int(t.get('/Size', 0))):
        try:
            o = r.get_object(num)
        except Exception:
            continue
        if o is None:
            continue
        where = f'obj {num}'
        d = o if isinstance(o, DictionaryObject) else None
        if d is None:
            continue
        sm = d.get('/SMask')
        if sm is not None and str(sm) != '/None':
            transparency.append(f'{where}: SMask')
        for k in ('/CA', '/ca'):
            if k in d and float(d[k]) < 1.0:
                transparency.append(f'{where}: {k} {d[k]}')
        if '/BM' in d:
            bm = res(d['/BM'])
            bms = [str(x) for x in bm] if isinstance(bm, ArrayObject) else [str(bm)]
            if any(b not in ('/Normal', '/Compatible') for b in bms):
                transparency.append(f'{where}: BM {bms}')
        g = res(d.get('/Group'))
        if isinstance(g, DictionaryObject) and g.get('/S') == '/Transparency':
            transparency.append(f'{where}: transparency group')
        if '/SMaskInData' in d and int(d['/SMaskInData']) != 0:
            transparency.append(f'{where}: SMaskInData')
        filt = res(d.get('/Filter'))
        fl = [str(x) for x in filt] if isinstance(filt, ArrayObject) else [str(filt)] if filt is not None else []
        for f in fl:
            if f in ('/JBIG2Decode', '/JPXDecode'):
                newer.append(f'{where}: {f}')
        if d.get('/Subtype') == '/Image' and d.get('/BitsPerComponent') == 16:
            newer.append(f'{where}: 16-bit image')
        if '/OC' in d:
            newer.append(f'{where}: optional content')
        for bad in ('/TR', '/TR2'):
            if bad in d and str(d[bad]) not in ('/Identity', '/Default'):
                forbidden[f'transfer function {bad}'] += 1
        if '/OPI' in d:
            forbidden['OPI'] += 1
        if d.get('/Subtype') == '/PS' or d.get('/Subtype2') == '/PS':
            forbidden['PostScript XObject'] += 1
        if d.get('/Subtype') == '/Image':
            if '/Alternates' in d:
                forbidden['image Alternates'] += 1
            if d.get('/Interpolate') is True:
                forbidden['image Interpolate true'] += 1
        if d.get('/S') == '/JavaScript' or '/JS' in d:
            forbidden['JavaScript action'] += 1
        scan_value(d, where)
    rep.check('no transparency (soft masks, alpha, blend modes, transparency groups) in any object', not transparency, shorten(transparency))
    rep.check('no PDF 1.4+ features (JBIG2, JPX, 16-bit images, optional content)', not newer, shorten(newer))
    rep.check('no transfer functions, OPI, PostScript XObjects, image Alternates/Interpolate, JavaScript', not forbidden, json.dumps(dict(forbidden)))

    # Pages, resources, content.
    box_problems, font_problems, colour, icc_problems, annots, devn, qdepth, ps_ops = [], [], [], [], [], [], [], []
    seen = set()
    fonts_total = 0
    stats = collections.Counter()

    def check_cs(cs, where, drgb):
        cs = res(cs)
        if cs is None:
            return
        if isinstance(cs, NameObject):
            n = str(cs)
            if n in ('/DeviceCMYK', '/DeviceGray', '/Pattern', '/CMYK', '/G'):
                return
            if n in ('/DeviceRGB', '/RGB'):
                if not x3 or not drgb:
                    colour.append(f'{where}: DeviceRGB')
                return
            if n.startswith('/') and n not in ('/Indexed',):
                return
            return
        if isinstance(cs, ArrayObject) and cs:
            k = str(cs[0])
            if k == '/ICCBased':
                s = res(cs[1])
                if not x3:
                    colour.append(f'{where}: ICCBased')
                    return
                h = icc_header(s.get_data())
                n = s.get('/N')
                if not h or {'CMYK': 4, 'RGB': 3, 'GRAY': 1, 'LAB': 3}.get(h['space'].upper()) != n:
                    icc_problems.append(f'{where}: N={n} header={h}')
                elif h['major'] > 2:
                    icc_problems.append(f'{where}: ICC version {h["version"]} (PDF 1.3 holds version 2)')
            elif k in ('/CalRGB', '/CalGray', '/Lab'):
                if not x3:
                    colour.append(f'{where}: {k[1:]}')
            elif k in ('/Separation', '/DeviceN'):
                alt = res(cs[2])
                if k == '/DeviceN':
                    n = len(res(cs[1]))
                    stats['DeviceN'] += 1
                    if n > 8:
                        devn.append(f'{where}: {n} colorants')
                else:
                    stats['Separation'] += 1
                if not x3 and not (isinstance(alt, NameObject) and str(alt) in ('/DeviceCMYK', '/DeviceGray')):
                    colour.append(f'{where}: {k[1:]} alternate {str(alt)[:40]}')
                check_cs(alt, where + ' alternate', drgb)
            elif k == '/Indexed':
                check_cs(cs[1], where + ' base', drgb)
            elif k == '/Pattern' and len(cs) > 1:
                check_cs(cs[1], where + ' pattern base', drgb)

    def scan_content(data, where, drgb):
        text = data.decode('latin-1')
        text = re.sub(r'\((?:\\.|[^\\)])*\)', '()', text)
        depth = mx = 0
        for tok in re.findall(r'(?<![\w/.])(q|Q|rg|RG|PS|BI)(?![\w*])', text):
            if tok == 'q':
                depth += 1
                mx = max(mx, depth)
            elif tok == 'Q':
                depth -= 1
            elif tok in ('rg', 'RG'):
                if not x3 or not drgb:
                    colour.append(f'{where}: {tok} operator')
            elif tok == 'PS':
                ps_ops.append(where)
        if mx > 28:
            qdepth.append(f'{where}: q nesting {mx}')
        for bi in re.finditer(r'BI\s(.*?)\sID\s', text, re.S):
            if re.search(r'/(CS|ColorSpace)\s*/(RGB|DeviceRGB|CalRGB|Lab)', bi.group(1)) and (not x3 or not drgb):
                colour.append(f'{where}: inline image in RGB')
        stats['content streams'] += 1

    def walk(rd, where, drgb, depth=0):
        nonlocal fonts_total
        rd = res(rd)
        if not isinstance(rd, DictionaryObject) or depth > 20:
            return
        cs = res(rd.get('/ColorSpace')) or {}
        drgb = drgb or ('/DefaultRGB' in cs)
        for k, v in cs.items():
            check_cs(v, f'{where} cs {k}', drgb)
        for k, v in (res(rd.get('/Font')) or {}).items():
            oid = v.idnum if isinstance(v, IndirectObject) else id(v)
            if ('f', oid) in seen:
                continue
            seen.add(('f', oid))
            fonts_total += 1
            f = res(v)
            st = f.get('/Subtype')
            if st == '/Type3':
                walk(f.get('/Resources'), f'{where} Type3 {k}', drgb, depth + 1)
                continue
            if st == '/Type0':
                f = res(res(f['/DescendantFonts'])[0])
            fd = res(f.get('/FontDescriptor'))
            if not fd or not any(x in fd for x in ('/FontFile', '/FontFile2', '/FontFile3')):
                font_problems.append(f"{where}: {f.get('/BaseFont')} not embedded")
            elif '/FontFile3' in fd and str(res(fd['/FontFile3']).get('/Subtype')) == '/OpenType':
                newer.append(f'{where}: OpenType font file (PDF 1.6)')
        for k, v in (res(rd.get('/XObject')) or {}).items():
            oid = v.idnum if isinstance(v, IndirectObject) else id(v)
            if ('x', oid, drgb) in seen:
                continue
            seen.add(('x', oid, drgb))
            x = res(v)
            if x.get('/Subtype') == '/Image':
                stats['images'] += 1
                if '/ColorSpace' in x:
                    check_cs(x['/ColorSpace'], f'{where} image {k}', drgb)
            elif x.get('/Subtype') == '/Form':
                stats['forms'] += 1
                sub = res(x.get('/Resources'))
                scan_content(x.get_data(), f'{where} form {k}', drgb or ('/DefaultRGB' in (res((sub or {}).get('/ColorSpace')) or {})))
                walk(sub, f'{where}/{k}', drgb, depth + 1)
        for k, v in (res(rd.get('/Shading')) or {}).items():
            check_cs(res(v).get('/ColorSpace'), f'{where} shading {k}', drgb)
        for k, v in (res(rd.get('/Pattern')) or {}).items():
            pt = res(v)
            if pt.get('/PatternType') == 2:
                check_cs(res(pt.get('/Shading')).get('/ColorSpace'), f'{where} pattern {k}', drgb)
            elif isinstance(pt, StreamObject):
                scan_content(pt.get_data(), f'{where} pattern {k}', drgb)
                walk(pt.get('/Resources'), f'{where} pattern {k}', drgb, depth + 1)

    for i, p in enumerate(r.pages):
        def box(k):
            v = p.get(k)
            return [float(x) for x in v] if v is not None else None
        mb, bb, tb, ab = box('/MediaBox'), box('/BleedBox'), box('/TrimBox'), box('/ArtBox')
        if not (tb or ab):
            box_problems.append(f'page {i}: no TrimBox or ArtBox')
        if tb and ab:
            box_problems.append(f'page {i}: both TrimBox and ArtBox')
        def inside(a, b, tol=0.01):
            return a[0] >= b[0] - tol and a[1] >= b[1] - tol and a[2] <= b[2] + tol and a[3] <= b[3] + tol
        if bb and mb and not inside(bb, mb):
            box_problems.append(f'page {i}: BleedBox outside MediaBox')
        if tb and (bb or mb) and not inside(tb, bb or mb):
            box_problems.append(f'page {i}: TrimBox outside BleedBox')
        rd = res(p.get('/Resources')) or {}
        drgb = '/DefaultRGB' in (res(rd.get('/ColorSpace')) or {})
        g = res(p.get('/Group'))
        if g is not None and g.get('/CS') is not None:
            check_cs(g.get('/CS'), f'page {i} group', drgb)
        c = p.get_contents()
        if c is not None:
            scan_content(c.get_data(), f'page {i}', drgb)
        walk(rd, f'page {i}', drgb)
        if '/AA' in p:
            forbidden['page additional actions'] += 1
        for a in res(p.get('/Annots')) or []:
            a = res(a)
            st = str(a.get('/Subtype'))
            if st in ('/PrinterMark', '/TrapNet'):
                continue
            rect = [float(x) for x in a.get('/Rect', [0, 0, 0, 0])]
            limit = bb or tb or mb
            if not (rect[2] <= limit[0] or rect[0] >= limit[2] or rect[3] <= limit[1] or rect[1] >= limit[3]):
                annots.append(f'page {i}: {st} inside the bleed box')

    rep.check('page boxes: TrimBox/ArtBox present and nested', not box_problems, shorten(box_problems))
    rep.check('all fonts embedded (no OpenType font files)', not font_problems, f'{fonts_total} fonts; ' + shorten(font_problems))
    if x3:
        rep.check('colour: no DeviceRGB without DefaultRGB', not colour, shorten(colour))
        rep.check('ICC-based profiles valid, N matches, ICC version 2', not icc_problems, shorten(icc_problems))
    else:
        rep.check('colour: only CMYK, gray, spot (CMYK/gray alternate), Indexed and Pattern', not colour, shorten(colour))
    rep.check('no PostScript operators', not ps_ops, shorten(ps_ops))
    rep.check('PDF 1.3 limits: q nesting <= 28', not qdepth, shorten(qdepth))
    rep.check('PDF 1.3 limits: DeviceN <= 8 colorants', not devn, shorten(devn))
    rep.check('PDF 1.3 limits: arrays, dictionaries, names, strings, integers', not limits, shorten(limits))
    rep.check('annotations only PrinterMark/TrapNet inside the bleed box', not annots, shorten(annots))
    out = {'file': path, 'standard': want_conf, 'pages': len(r.pages), 'summary': dict(rep.counts), 'stats': dict(stats), 'checks': rep.items}
    return out


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--x3']
    o = main(args[0], '--x3' in sys.argv)
    if len(args) > 1:
        open(args[1], 'w', encoding='utf-8').write(json.dumps(o, indent=1, ensure_ascii=False))
    print(json.dumps(o['summary']), o['standard'], o['file'])
    for c in o['checks']:
        if c['status'] != 'pass':
            print(' ', c['status'].upper(), c['check'], '|', c['detail'][:300])
