"""PDF/X-4 (ISO 15930-7:2010) structure check of the main requirements.

Usage: python pdfx4check.py file.pdf [out.json]

Checks: header version; no encryption; trailer ID; one GTS_PDFX output intent with an
embedded ICC output profile (header valid, N and colour space agree, output class);
OutputConditionIdentifier; XMP (pdfxid:GTS_PDFXVersion = PDF/X-4, dc:title, CreateDate,
ModifyDate, CreatorTool, pdf:Producer, pdf:Trapped True/False, xmpMM DocumentID / VersionID /
RenditionClass) and its consistency with the Info dictionary; page boxes (TrimBox or ArtBox,
nesting MediaBox ⊇ BleedBox ⊇ TrimBox); every font embedded; no DeviceRGB without DefaultRGB
in a CMYK-intent file (operators rg/RG, images, colour spaces); ICCBased N matches the profile;
no transfer functions (TR/TR2), OPI, PostScript XObjects, image Alternates; no JavaScript,
embedded files or forbidden actions; annotations only PrinterMark/TrapNet inside the bleed box.
This is a structural check, not a certified validator.
"""
import sys, json, re, collections
sys.stdout.reconfigure(encoding='utf-8')
from pypdf import PdfReader
from pypdf.generic import IndirectObject, ArrayObject, DictionaryObject, NameObject, StreamObject


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
    return {'size': int.from_bytes(data[0:4], 'big'), 'version': f'{data[8]}.{data[9] >> 4}', 'class': data[12:16].decode('latin-1'),
            'space': data[16:20].decode('latin-1').strip(), 'pcs': data[20:24].decode('latin-1').strip()}


def main(path):
    rep = Report()
    raw = open(path, 'rb').read()
    head = raw[:16]
    m = re.match(rb'%PDF-(\d)\.(\d)', head)
    version = (int(m.group(1)), int(m.group(2))) if m else None
    rep.check('header PDF version <= 1.6', version is not None and version <= (1, 6), f'header {head[:8]!r}')
    r = PdfReader(path)
    t = r.trailer
    rep.check('not encrypted', '/Encrypt' not in t)
    ids = t.get('/ID')
    rep.check('trailer /ID present (two strings)', isinstance(ids, ArrayObject) and len(ids) == 2)
    cat = res(t['/Root'])

    # Output intent.
    ois = res(cat.get('/OutputIntents')) or []
    gts = [res(o) for o in ois if res(o).get('/S') == '/GTS_PDFX']
    rep.check('exactly one GTS_PDFX output intent', len(gts) == 1, f'{len(gts)} found')
    profile_space = None
    if gts:
        oi = gts[0]
        rep.check('OutputConditionIdentifier present', bool(oi.get('/OutputConditionIdentifier')), str(oi.get('/OutputConditionIdentifier')))
        prof = res(oi.get('/DestOutputProfile'))
        rep.check('DestOutputProfile embedded', isinstance(prof, StreamObject))
        if isinstance(prof, StreamObject):
            data = prof.get_data()
            h = icc_header(data)
            rep.check('output profile is a valid ICC profile', h is not None, json.dumps(h))
            if h:
                profile_space = h['space']
                rep.check('output profile N matches its colour space', {'CMYK': 4, 'RGB': 3, 'GRAY': 1}.get(h['space'].upper()) == prof.get('/N'), f"N={prof.get('/N')} space={h['space']}")
                rep.check('output profile class is output (prtr) or monitor/colour space', h['class'] in ('prtr', 'mntr', 'scnr', 'spac'), h['class'])
                rep.check('output profile version 2 or 4', h['version'][0] in '24', h['version'])
        rep.check('RegistryName present for a registered identifier', bool(oi.get('/RegistryName')) or str(oi.get('/OutputConditionIdentifier')) == 'Custom', str(oi.get('/RegistryName')), level='warn')

    # Metadata.
    md = res(cat.get('/Metadata'))
    xmp = md.get_data().decode('utf-8', 'replace') if isinstance(md, StreamObject) else ''
    rep.check('catalog Metadata stream (XMP)', bool(xmp))
    def xget(tag):
        mm = re.search(r'<' + tag + r'>(.*?)</' + tag + r'>', xmp, re.S) or re.search(tag + r'="([^"]*)"', xmp)
        return re.sub(r'<[^>]+>', '', mm.group(1)).strip() if mm else None
    rep.check('XMP pdfxid:GTS_PDFXVersion = PDF/X-4', xget('pdfxid:GTS_PDFXVersion') == 'PDF/X-4', str(xget('pdfxid:GTS_PDFXVersion')))
    for tag in ('dc:title', 'xmp:CreateDate', 'xmp:ModifyDate', 'xmp:CreatorTool', 'pdf:Producer', 'xmpMM:DocumentID', 'xmpMM:VersionID', 'xmpMM:RenditionClass'):
        rep.check(f'XMP {tag}', bool(xget(tag)), str(xget(tag))[:80], level='fail' if tag in ('dc:title', 'xmp:CreateDate', 'xmp:ModifyDate', 'xmpMM:DocumentID', 'xmpMM:VersionID') else 'warn')
    rep.check('XMP pdf:Trapped True or False', xget('pdf:Trapped') in ('True', 'False'), str(xget('pdf:Trapped')))
    info = res(t.get('/Info'))
    if info:
        tr = info.get('/Trapped')
        rep.check('Info Trapped /True or /False', str(tr) in ('/True', '/False'), str(tr))
        if xget('pdf:Trapped') and tr is not None:
            rep.check('Info Trapped agrees with XMP', str(tr).lstrip('/') == xget('pdf:Trapped'), f'{tr} vs {xget("pdf:Trapped")}')
        if info.get('/Title') is not None and xget('dc:title'):
            rep.check('Info Title agrees with XMP dc:title', str(info.get('/Title')) == xget('dc:title'), f"{str(info.get('/Title'))[:60]!r} vs {xget('dc:title')[:60]!r}", level='warn')
        if info.get('/Producer') is not None and xget('pdf:Producer'):
            rep.check('Info Producer agrees with XMP', str(info.get('/Producer')) == xget('pdf:Producer'), '', level='warn')
        gv = info.get('/GTS_PDFXVersion')
        if gv is not None:
            rep.check('Info GTS_PDFXVersion (if present) is PDF/X-4', str(gv).startswith('PDF/X-4'), str(gv))

    # Document-level forbidden features.
    names = res(cat.get('/Names')) or {}
    rep.check('no JavaScript name tree', '/JavaScript' not in names)
    rep.check('no embedded files', '/EmbeddedFiles' not in names)
    rep.check('no document OpenAction JavaScript', not (cat.get('/OpenAction') and 'JavaScript' in str(res(cat.get('/OpenAction')))))
    rep.check('no catalog additional actions', '/AA' not in cat)

    # Pages and resources.
    box_problems, font_problems, rgb_uses, icc_problems, forbidden, annots = [], [], [], [], collections.Counter(), []
    seen = set()
    fonts_total = 0

    def walk_resources(rd, where, default_rgb):
        nonlocal fonts_total
        rd = res(rd)
        if not isinstance(rd, DictionaryObject):
            return
        key = id(rd)
        cs = res(rd.get('/ColorSpace')) or {}
        drgb = default_rgb or ('/DefaultRGB' in cs)
        for k, v in cs.items():
            check_cs(res(v), f'{where} cs {k}', drgb)
        for k, v in (res(rd.get('/ExtGState')) or {}).items():
            g = res(v)
            for bad in ('/TR', '/TR2'):
                if bad in g and str(g[bad]) not in ('/Identity', '/Default'):
                    forbidden[f'ExtGState {bad}'] += 1
            if '/HTP' in g:
                forbidden['ExtGState HTP'] += 1
        for k, v in (res(rd.get('/Font')) or {}).items():
            oid = v.idnum if isinstance(v, IndirectObject) else id(v)
            if ('f', oid) in seen:
                continue
            seen.add(('f', oid))
            fonts_total += 1
            f = res(v)
            st = f.get('/Subtype')
            if st == '/Type3':
                continue
            if st == '/Type0':
                f = res(res(f['/DescendantFonts'])[0])
            fd = res(f.get('/FontDescriptor'))
            if not fd or not any(x in fd for x in ('/FontFile', '/FontFile2', '/FontFile3')):
                font_problems.append(f"{where}: {f.get('/BaseFont')} not embedded")
        for k, v in (res(rd.get('/XObject')) or {}).items():
            oid = v.idnum if isinstance(v, IndirectObject) else id(v)
            x = res(v)
            if ('x', oid, drgb) in seen:
                continue
            seen.add(('x', oid, drgb))
            if x.get('/Subtype') == '/Image':
                if '/OPI' in x:
                    forbidden['image OPI'] += 1
                if '/Alternates' in x:
                    forbidden['image Alternates'] += 1
                if x.get('/Interpolate') is True:
                    forbidden['image Interpolate true'] += 1
                if '/ColorSpace' in x:
                    check_cs(res(x['/ColorSpace']), f'{where} image {k}', drgb)
                sm = res(x.get('/SMask'))
                if isinstance(sm, StreamObject) and '/ColorSpace' in sm:
                    check_cs(res(sm['/ColorSpace']), f'{where} smask {k}', drgb)
            elif x.get('/Subtype') == '/PS':
                forbidden['PostScript XObject'] += 1
            elif x.get('/Subtype') == '/Form':
                if '/OPI' in x:
                    forbidden['form OPI'] += 1
                if x.get('/Subtype2') == '/PS':
                    forbidden['PostScript form'] += 1
                sub_rd = res(x.get('/Resources'))
                scan_content(x.get_data(), f'{where} form {k}', drgb or ('/DefaultRGB' in (res((sub_rd or {}).get('/ColorSpace')) or {})))
                walk_resources(sub_rd, f'{where}/{k}', drgb)
        for k, v in (res(rd.get('/Shading')) or {}).items():
            s = res(v)
            check_cs(res(s.get('/ColorSpace')), f'{where} shading {k}', drgb)
        for k, v in (res(rd.get('/Pattern')) or {}).items():
            pt = res(v)
            if pt.get('/PatternType') == 2:
                check_cs(res(res(pt.get('/Shading')).get('/ColorSpace')), f'{where} pattern {k}', drgb)
            elif isinstance(pt, StreamObject):
                walk_resources(pt.get('/Resources'), f'{where} pattern {k}', drgb)

    def check_cs(cs, where, drgb):
        if cs is None:
            return
        if isinstance(cs, NameObject):
            if cs == '/DeviceRGB' and not drgb and profile_space and profile_space.upper() != 'RGB':
                rgb_uses.append(where)
            return
        if isinstance(cs, ArrayObject) and cs:
            k = str(cs[0])
            if k == '/ICCBased':
                s = res(cs[1])
                h = icc_header(s.get_data())
                n = s.get('/N')
                if not h or {'CMYK': 4, 'RGB': 3, 'GRAY': 1, 'LAB': 3}.get(h['space'].upper()) != n:
                    icc_problems.append(f"{where}: N={n} header={h}")
            elif k in ('/Separation', '/DeviceN'):
                check_cs(res(cs[2]), where + ' alternate', drgb)
            elif k == '/Indexed':
                check_cs(res(cs[1]), where + ' base', drgb)

    def scan_content(data, where, drgb):
        if drgb or not profile_space or profile_space.upper() == 'RGB':
            return
        text = data.decode('latin-1')
        # strip strings to avoid false matches inside text
        text = re.sub(r'\((?:\\.|[^\\)])*\)', '()', text)
        if re.search(r'(^|\s)[-\d.]+\s+[-\d.]+\s+[-\d.]+\s+(rg|RG)(\s|$)', text):
            rgb_uses.append(f'{where}: rg/RG operator')
        if re.search(r'/DeviceRGB\s+(cs|CS)', text):
            rgb_uses.append(f'{where}: /DeviceRGB cs')

    for i, p in enumerate(r.pages):
        def box(k):
            v = p.get(k)
            return [float(x) for x in v] if v is not None else None
        mb, bb, tb, ab = box('/MediaBox'), box('/BleedBox'), box('/TrimBox'), box('/ArtBox')
        cb = box('/CropBox')
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
        if cb and mb and not inside(cb, mb):
            box_problems.append(f'page {i}: CropBox outside MediaBox')
        rd = res(p.get('/Resources')) or {}
        drgb = '/DefaultRGB' in (res(rd.get('/ColorSpace')) or {})
        g = res(p.get('/Group'))
        if g and g.get('/CS') is not None:
            check_cs(res(g.get('/CS')), f'page {i} group', drgb)
        c = p.get_contents()
        if c is not None:
            scan_content(c.get_data(), f'page {i}', drgb)
        walk_resources(rd, f'page {i}', drgb)
        if '/AA' in p:
            forbidden['page additional actions'] += 1
        for a in res(p.get('/Annots')) or []:
            a = res(a)
            st = str(a.get('/Subtype'))
            if st in ('/PrinterMark', '/TrapNet'):
                continue
            rect = [float(x) for x in a.get('/Rect', [0, 0, 0, 0])]
            limit = bb or tb or mb
            overlaps = not (rect[2] <= limit[0] or rect[0] >= limit[2] or rect[3] <= limit[1] or rect[1] >= limit[3])
            if overlaps:
                annots.append(f'page {i}: {st} inside the bleed box')
            if st in ('/Widget', '/Link') and '/A' in a and 'JavaScript' in str(res(a['/A'])):
                forbidden['JavaScript action'] += 1

    rep.check('page boxes: TrimBox/ArtBox present and nested', not box_problems, '; '.join(box_problems[:5]) + (f' (+{len(box_problems) - 5})' if len(box_problems) > 5 else ''))
    rep.check('all fonts embedded', not font_problems, f'{fonts_total} fonts; ' + '; '.join(font_problems[:5]))
    rep.check('no DeviceRGB without DefaultRGB (CMYK output intent)', not rgb_uses, '; '.join(rgb_uses[:5]) + (f' (+{len(rgb_uses) - 5})' if len(rgb_uses) > 5 else ''))
    rep.check('ICCBased profiles valid, N matches', not icc_problems, '; '.join(icc_problems[:5]))
    rep.check('no transfer functions, OPI, PostScript, image Alternates/Interpolate, JavaScript actions', not forbidden, json.dumps(dict(forbidden)))
    rep.check('annotations only PrinterMark/TrapNet inside the bleed box', not annots, '; '.join(annots[:5]))
    out = {'file': path, 'pages': len(r.pages), 'summary': dict(rep.counts), 'checks': rep.items}
    return out


if __name__ == '__main__':
    o = main(sys.argv[1])
    if len(sys.argv) > 2:
        open(sys.argv[2], 'w', encoding='utf-8').write(json.dumps(o, indent=1, ensure_ascii=False))
    print(json.dumps(o['summary']), o['file'])
    for c in o['checks']:
        if c['status'] != 'pass':
            print(' ', c['status'].upper(), c['check'], '|', c['detail'][:300])
