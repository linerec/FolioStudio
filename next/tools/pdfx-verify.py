"""Small, dependency-free PDF/X structural audit; not ISO certification.

Usage: python -I next/tools/pdfx-verify.py file.pdf [--json] [--report-only] [--min-ppi N] [--min-bitmap-ppi N]
Errors are definite violations; warnings are things to look at. What could not be inspected is never a vague warning: the result's
`status` is "checked", "partial" or "unchecked" and `unchecked` lists where and why (unsupported filter, incremental update, encryption,
a font program kind). `images` lists every drawn picture with its effective ppi (pixels over the drawn size, from the CTM, through forms
and soft-mask groups; ISO 15930 sets no minimum, so only --min-ppi / --min-bitmap-ppi warn); `fonts` lists the used fonts with
glyph coverage (every used glyph must exist, with an outline unless it is a space, in the embedded font program).
Sources: ISO 15930-7:2010 (PDF/X-4, based on PDF 1.6), ISO 15930-1:2001
and ISO 15930-4:2003 (PDF/X-1a); ISO 32000-1:2008, 7.3/7.5/7.9.4
(objects, file structure, dates), 8.9 (images), 9.6 (fonts).
Public interpretation: https://pdfa.org/technical-side-and-requirements-of-pdfx/
https://www.loc.gov/preservation/digital/formats/fdd/fdd000124.shtml
Clause numbers not available in the public ISO 15930 summaries are not guessed.
"""
import argparse
import datetime as dt
import json
import math
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET
import zlib

# The audit also runs under -I and is loaded by file path from print-check.
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
try:
    from next.tools.pdfx_fonts import CFF, CFFOnly, TrueType, to_unicode, u16, u32
    from next.tools.pdfx_parser import (
        PDF, Name, Reader, Ref, Stream, Unsupported, decoded, lzw_decode, undo_predictor,
    )
finally:
    sys.path.pop(0)


def text(value):
    if isinstance(value, bytes):
        return value.decode('utf-16' if value.startswith((b'\xfe\xff', b'\xff\xfe')) else 'latin1')
    return str(value) if value is not None else ''


def date(value):
    s = text(value)
    if s.startswith('D:'):
        m = re.fullmatch(r'D:(\d{14})(Z|[+-]\d{2}\x27?\d{2}\x27?)?', s)
        if not m:
            return None
        zone = (m[2] or '').replace("'", '')
        s = dt.datetime.strptime(m[1], '%Y%m%d%H%M%S').isoformat()+zone
    try:
        return dt.datetime.fromisoformat(s.replace('Z', '+00:00'))
    except ValueError:
        return None


INLINE_SPACES = {'G': 1, 'DeviceGray': 1, 'CalGray': 1, 'RGB': 3, 'DeviceRGB': 3, 'CalRGB': 3, 'CMYK': 4, 'DeviceCMYK': 4}
INLINE_KEYS = {'W': 'Width', 'H': 'Height', 'BPC': 'BitsPerComponent', 'CS': 'ColorSpace', 'F': 'Filter', 'IM': 'ImageMask',
               'D': 'Decode', 'DP': 'DecodeParms', 'I': 'Interpolate'}


def inline_image(r):
    """Parses `BI <dict> ID <data> EI` (ISO 32000-1 8.9.7) and returns the dictionary with full key names."""
    d = {}
    while True:
        r.skip()
        if r.data[r.pos:r.pos+2] == b'ID' and (r.pos + 2 >= len(r.data) or r.data[r.pos+2] in b'\x00\t\n\f\r '):
            r.pos += 2
            break
        key = r.value()
        if not isinstance(key, Name):
            raise ValueError('inline image key is not a name')
        d[INLINE_KEYS.get(key, key)] = r.value()
    r.pos += 2 if r.data[r.pos:r.pos+2] == b'\r\n' else 1
    filters = d.get('Filter')
    if filters:
        match = re.compile(rb'[\x00\t\n\f\r ]EI(?![^\x00\t\n\f\r ])').search(r.data, r.pos)
        if not match:
            raise Unsupported('inline image without a recognizable EI')
        r.pos = match.end()
        return d
    space = d.get('ColorSpace')
    comps = 1 if d.get('ImageMask') else INLINE_SPACES.get(space)
    if comps is None and isinstance(space, list) and space and space[0] in ('I', 'Indexed'):
        comps = 1
    width, height, bpc = d.get('Width'), d.get('Height'), 1 if d.get('ImageMask') else d.get('BitsPerComponent')
    if comps is None or not all(isinstance(v, int) and v > 0 for v in (width, height, bpc)):
        raise Unsupported('inline image with a named colour space')
    r.pos += ((width * comps * bpc + 7) // 8) * height
    r.skip()
    if r.token() != b'EI':
        raise ValueError('inline image data does not end at EI')
    return d


def content_ops(data):
    """Yields (operator, operands) of a content stream; strings, names and comments cannot pose as operators."""
    r, stack = Reader(data), []
    while True:
        r.skip()
        if r.pos >= len(data):
            return
        pos = r.pos
        try:
            stack.append(r.value())
            continue
        except (ValueError, IndexError):
            r.pos = pos
        op = r.token()
        yield op, ([inline_image(r)] if op == b'BI' else stack)
        stack = []


IDENTITY = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def matrix_then(m, n):
    """Row-vector product: `m` is applied first, then `n` (PDF `cm` concatenates M x CTM)."""
    return (m[0]*n[0] + m[1]*n[2], m[0]*n[1] + m[1]*n[3], m[2]*n[0] + m[3]*n[2], m[2]*n[1] + m[3]*n[3],
            m[4]*n[0] + m[5]*n[2] + n[4], m[4]*n[1] + m[5]*n[3] + n[5])


def number_matrix(values):
    if len(values) == 6 and all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) for v in values):
        return tuple(float(v) for v in values)
    return None


class Audit:
    def __init__(self, data, limits=None):
        self.data, self.issues, self.pages = data, [], 0
        self.variant, self.space = 'unknown', None
        self.limits = limits or {}
        self.images, self.fonts, self.unchecked = [], [], []
        self.image_refs, self.mask_refs, self.placed, self.font_uses = set(), set(), set(), {}
        self.ops_cache = {}

    def issue(self, code, detail, warning=False):
        row = {'level': 'warning' if warning else 'error', 'code': code, 'detail': str(detail)}
        if row not in self.issues:
            self.issues.append(row)

    def check(self, ok, code, detail, warning=False):
        if not ok:
            self.issue(code, detail, warning)

    def run(self):
        self.pdf = PDF(self.data)
        self.res = self.pdf.resolve
        t = self.pdf.trailer
        self.check('Encrypt' not in t, 'encryption', 'PDF/X prohibits encryption')
        if 'Encrypt' in t:
            raise Unsupported('encrypted file: its strings and streams cannot be read, so nothing but the encryption itself was inspected')
        ids = self.res(t.get('ID'))
        self.check(isinstance(ids, list) and len(ids) == 2 and all(isinstance(x, bytes) and x for x in ids), 'trailer.id', 'ID must contain two nonempty strings')
        cat = self.res(t['Root'])
        self.metadata(cat, self.res(t.get('Info')) or {})
        if self.variant.startswith('PDF/X-3:'):
            raise Unsupported('PDF/X-3 file: its identification and metadata were audited, but the colour and transparency rules of ISO 15930-3 are not checked')
        header = re.match(rb'%PDF-(\d+)\.(\d+)', self.data)
        version = tuple(map(int, header.groups())) if header else None
        ceiling = (1, 3) if self.variant == 'PDF/X-1a:2001' else (1, 4) if '1a' in self.variant else (1, 6)
        self.check(version and (1, 0) <= version <= ceiling, 'header.version', f'{version}; expected <= {ceiling}')
        self.output(cat)
        for key, obj in list(self.pdf.objects.items()):
            self.current = key
            self.walk(obj, f'object {key[0]}')
        self.page_tree(cat.get('Pages'), {}, set())
        self.duplicates()
        self.finish()

    def metadata(self, cat, info):
        md = self.res(cat.get('Metadata'))
        props = {}
        if isinstance(md, Stream):
            root = ET.fromstring(decoded(md))
            for el in root.iter():
                props[el.tag] = ''.join(el.itertext()).strip()
                props.update(el.attrib)
        pdfx = props.get('{http://www.npes.org/pdfx/ns/id/}GTS_PDFXVersion', '')
        identification = pdfx or text(info.get('GTS_PDFXVersion'))
        conformance = text(info.get('GTS_PDFXConformance')) or props.get('{http://ns.adobe.com/pdfx/1.3/}GTS_PDFXConformance', '')
        self.variant = conformance if conformance.startswith('PDF/X-1a:') else identification or 'unknown'
        self.check(self.variant in ('PDF/X-4', 'PDF/X-1a:2001', 'PDF/X-1a:2003') or self.variant.startswith('PDF/X-3:'), 'pdfx.version', self.variant)
        x4 = self.variant == 'PDF/X-4'
        self.check(bool(pdfx), 'xmp.pdfx_version', 'XMP PDF/X identification missing', not x4)
        # ISO 15930-7 metadata: Info is optional in X-4; ISO 15930-1/-4 use Info.
        for key in ('GTS_PDFXVersion', 'Title', 'CreationDate', 'ModDate', 'Trapped'):
            self.check(key in info, 'info.'+key, 'Info /'+key+' absent', x4)
        if 'GTS_PDFXVersion' in info:
            self.check(text(info['GTS_PDFXVersion']) == identification, 'info.version_mismatch', 'Info and XMP identification differ')
        xmp_conformance = props.get('{http://ns.adobe.com/pdfx/1.3/}GTS_PDFXConformance')
        if xmp_conformance and 'GTS_PDFXConformance' in info:
            self.check(xmp_conformance == text(info['GTS_PDFXConformance']), 'info.conformance_mismatch', 'Info and XMP PDF/X conformance differ')
        trapped = info.get('Trapped', props.get('{http://ns.adobe.com/pdf/1.3/}Trapped'))
        self.check(trapped in ('True', 'False'), 'info.trapped', f'Trapped={trapped}')
        for key, tag in (('CreationDate', 'CreateDate'), ('ModDate', 'ModifyDate')):
            value = props.get('{http://ns.adobe.com/xap/1.0/}'+tag)
            self.check(bool(value), 'xmp.'+tag, 'XMP '+tag+' absent', not x4)
            if value and key in info:
                a, b = date(info[key]), date(value)
                # ISO 32000-1 7.9.4 allows partial dates/timezones: unresolved precision warns.
                self.check(a is not None and b is not None, 'date.precision', key+' cannot be compared at full precision', True)
                if a is not None and b is not None:
                    if (a.tzinfo is None) != (b.tzinfo is None):
                        self.issue('date.timezone', key+' has unspecified timezone', True)
                    else:
                        self.check(a == b, 'date.mismatch', key+' disagrees with XMP')

    def output(self, cat):
        intents = self.res(cat.get('OutputIntents')) or []
        intents = [self.res(x) for x in intents]
        intents = [x for x in intents if x.get('S') == 'GTS_PDFX']
        self.check(len(intents) == 1, 'output.intent', f'{len(intents)} GTS_PDFX output intents')
        for oi in intents:
            self.check(bool(self.res(oi.get('OutputConditionIdentifier'))), 'output.identifier', 'OutputConditionIdentifier missing')
            profile = self.res(oi.get('DestOutputProfile'))
            # ISO 15930-1 allows registered characterized output without an embedded profile.
            self.check(isinstance(profile, Stream), 'output.profile', 'embedded ICC stream missing', '1a' in self.variant)
            if not isinstance(profile, Stream):
                continue
            data = decoded(profile)
            good = len(data) >= 128 and data[36:40] == b'acsp' and int.from_bytes(data[:4], 'big') == len(data)
            self.check(good, 'output.icc_header', 'invalid ICC signature/length')
            if good:
                self.space = data[16:20].decode('latin1').strip()
                self.check(self.space in ('CMYK', 'RGB', 'GRAY'), 'output.color_space', self.space)
                self.check('1a' not in self.variant or self.space in ('CMYK', 'GRAY'), 'output.color_x1a', self.space)
                self.check(profile.get('N') == {'CMYK':4, 'RGB':3, 'GRAY':1}.get(self.space), 'output.components', f'ICC {self.space}, N={profile.get("N")}')

    def walk(self, obj, where):
        if isinstance(obj, Ref):
            self.res(obj)  # Check every reference, including references outside page resources.
        elif isinstance(obj, list):
            for value in obj:
                self.walk(value, where)
        elif isinstance(obj, dict):
            for key, value in obj.items():
                if key in ('JavaScript', 'EmbeddedFiles') or key == 'S' and value in ('JavaScript', 'Launch'):
                    self.issue('forbidden.'+str(value if key == 'S' else key), where)
                self.walk(value, where)
            if '1a' in self.variant:
                transparent = obj.get('S') == 'Transparency' or obj.get('SMask', Name('None')) != 'None'
                transparent |= any(isinstance(obj.get(k), (int, float)) and obj[k] < 1 for k in ('ca', 'CA'))
                modes = self.res(obj.get('BM', Name('Normal')))
                modes = modes if isinstance(modes, list) else [modes]
                transparent |= any(mode not in ('Normal', 'Compatible') for mode in modes)
                self.check(not transparent, 'transparency.x1a', where)
            if obj.get('Type') == 'Font':
                if obj.get('Subtype') == 'Type0':
                    descendants = self.res(obj.get('DescendantFonts'))
                    self.check(isinstance(descendants, list) and len(descendants) == 1, 'font.descendant', where+' Type0 requires one descendant font')
                elif obj.get('Subtype') == 'Type3':
                    self.check(bool(self.res(obj.get('CharProcs'))), 'font.embedded', where+' Type3 CharProcs absent')
                else:
                    fd = self.res(obj.get('FontDescriptor')) or {}
                    programs = [self.res(fd.get(k)) for k in ('FontFile', 'FontFile2', 'FontFile3')]
                    programs = [x for x in programs if isinstance(x, Stream)]
                    self.check(bool(programs), 'font.embedded', where+' '+text(obj.get('BaseFont')))
                    self.check(all(x.data for x in programs), 'font.program', where+' '+text(obj.get('BaseFont'))+' font program stream is empty')
            if obj.get('Subtype') == 'Image':
                self.image(obj, where)

    def image(self, obj, where):
        self.check(all(isinstance(obj.get(k), int) and obj[k] > 0 for k in ('Width', 'Height')), 'image.dimensions', where)
        filters = self.res(obj.get('Filter')) or []
        filters = filters if isinstance(filters, list) else [filters]
        allowed = {'ASCIIHexDecode', 'ASCII85Decode', 'LZWDecode', 'FlateDecode', 'RunLengthDecode', 'CCITTFaxDecode', 'DCTDecode', 'JBIG2Decode', 'JPXDecode'}
        for f in filters:
            self.check(f in allowed, 'image.filter', where+' '+str(f))
            self.check(not ('1a' in self.variant and f in ('JPXDecode', 'JBIG2Decode')), 'image.filter_x1a', where+' '+str(f), True)
        # ISO 15930 sets no minimum resolution; the effective ppi comes from where the picture is drawn (scan/place).
        self.image_refs.add(self.current)
        for key in ('SMask', 'Mask'):
            if isinstance(obj.get(key), Ref):
                self.mask_refs.add(obj[key])

    def page_tree(self, ref, inherited, seen):
        node = self.res(ref)
        if id(node) in seen:
            raise ValueError('cyclic/repeated page tree node')
        seen.add(id(node))
        state = dict(inherited)
        state.update({k: self.res(node[k]) for k in ('MediaBox', 'CropBox', 'Resources') if k in node})
        if node.get('Type') == 'Pages':
            for child in self.res(node.get('Kids')) or []:
                self.page_tree(child, state, seen)
            return
        self.pages += 1
        where = f'page {self.pages}'
        media = state.get('MediaBox')
        trim = self.res(node.get('TrimBox', node.get('ArtBox')))
        bleed = self.res(node.get('BleedBox', state.get('CropBox', media)))
        self.check(not ('TrimBox' in node and 'ArtBox' in node), 'box.trim_art', where)
        boxes = [media, bleed, trim]
        valid = all(isinstance(b, list) and len(b) == 4 and all(isinstance(x, (int, float)) and math.isfinite(x) for x in b) and b[0] < b[2] and b[1] < b[3] for b in boxes)
        self.check(valid, 'box.required', where+' missing/invalid MediaBox, TrimBox/ArtBox or BleedBox')
        if valid:
            contains = lambda a, b: a[0] <= b[0]+0.001 and a[1] <= b[1]+0.001 and a[2]+0.001 >= b[2] and a[3]+0.001 >= b[3]
            self.check(contains(media, bleed) and contains(bleed, trim), 'box.nesting', where)
        # BleedBox defaults to CropBox in PDF: absence alone is not an ISO violation.
        self.check('BleedBox' in node, 'box.bleed_default', where+' uses default BleedBox', True)
        self.ops_cache = {}
        self.resources(state.get('Resources') or {}, node.get('Contents'), where, set())
        self.scan(state.get('Resources') or {}, self.streams(node.get('Contents')), IDENTITY, None, where, frozenset())

    def resources(self, resources, contents, where, seen):
        resources = self.res(resources)
        spaces = self.res(resources.get('ColorSpace')) or {}
        def color(value):
            value = self.res(value)
            if isinstance(value, Name) and value in spaces:
                value = self.res(spaces[value])
            if isinstance(value, list):
                if value and value[0] == 'ICCBased':
                    profile = self.res(value[1])
                    data = decoded(profile)
                    self.check(len(data) >= 128 and data[36:40] == b'acsp' and profile.get('N') == {b'CMYK':4, b'RGB ':3, b'GRAY':1}.get(data[16:20]), 'color.icc', where+' ICCBased profile/components')
                    # ISO 15930-1/-4: PDF/X-1a allows only DeviceGray, DeviceCMYK and spot colour spaces.
                    self.check('1a' not in self.variant, 'color.x1a', where+' ICCBased colour space is not allowed in PDF/X-1a')
                if value and value[0] in ('CalRGB', 'CalGray', 'Lab'):
                    self.check('1a' not in self.variant, 'color.x1a', where+' '+str(value[0])+' colour space is not allowed in PDF/X-1a')
                if value and value[0] in ('Indexed', 'Pattern') and len(value) > 1:
                    color(value[1])
                if value and value[0] in ('Separation', 'DeviceN') and len(value) > 2 and '1a' in self.variant:
                    color(value[2])       # the alternate space must be CMYK or gray too
                return
            if value in ('DeviceCMYK', 'DeviceRGB', 'DeviceGray'):
                actual = {'DeviceCMYK':'CMYK', 'DeviceRGB':'RGB', 'DeviceGray':'GRAY'}[value]
                default = 'Default'+value[6:]
                compatible = actual == self.space or actual == 'GRAY' and self.space in ('CMYK', 'RGB') or default in spaces and self.variant == 'PDF/X-4'
                self.check(compatible, 'color.device', where+' '+value+' with '+str(self.space)+' output')
        for stream in self.streams(contents):
            try:
                # Operators come from the tokenizer, so strings/comments cannot masquerade as operators.
                for op, stack in self.ops(stream):
                    if op in (b'rg', b'RG', b'k', b'K', b'g', b'G'):
                        color(Name({b'rg':'DeviceRGB', b'RG':'DeviceRGB', b'k':'DeviceCMYK', b'K':'DeviceCMYK', b'g':'DeviceGray', b'G':'DeviceGray'}[op]))
                    if op in (b'cs', b'CS') and stack:
                        color(stack[-1])
                    if op == b'BI' and stack and isinstance(stack[0], dict) and 'ColorSpace' in stack[0]:
                        space = stack[0]['ColorSpace']
                        color(Name({'G': 'DeviceGray', 'RGB': 'DeviceRGB', 'CMYK': 'DeviceCMYK'}.get(space, space)) if isinstance(space, Name) else space)
            except Unsupported as e:
                self.unsupported(where, e)
        for obj in (self.res(resources.get('XObject')) or {}).values():
            obj = self.res(obj)
            if id(obj) in seen:
                continue
            seen.add(id(obj))
            if 'ColorSpace' in obj:
                color(obj['ColorSpace'])
            if obj.get('Subtype') == 'Form':
                self.resources(self.res(obj.get('Resources')) or resources, obj, where+' form', seen)
        for obj in (self.res(resources.get('Shading')) or {}).values():
            color(self.res(obj).get('ColorSpace'))
        for obj in (self.res(resources.get('Pattern')) or {}).values():
            obj = self.res(obj)
            if id(obj) in seen:
                continue
            seen.add(id(obj))
            if obj.get('PatternType') == 1:
                self.resources(self.res(obj.get('Resources')) or resources, obj, where+' pattern', seen)
            elif obj.get('PatternType') == 2:
                color(self.res(obj['Shading']).get('ColorSpace'))

    # ---- content scan: where pictures are drawn (CTM) and which codes each font shows ----------------------

    def unsupported(self, where, error):
        self.issue('coverage.content', where+' '+str(error), True)
        row = {'where': where, 'reason': str(error)}
        if row not in self.unchecked:
            self.unchecked.append(row)

    def streams(self, contents):
        streams = self.res(contents) if contents is not None else []
        streams = streams if isinstance(streams, list) else [streams]
        return [self.res(x) for x in streams if isinstance(self.res(x), Stream)]

    def ops(self, stream):
        """Operators of one content stream, parsed once per page (colour pass and placement pass share them)."""
        if id(stream) not in self.ops_cache:
            self.ops_cache[id(stream)] = (stream, list(content_ops(decoded(stream))))
        return self.ops_cache[id(stream)][1]

    def scan(self, resources, streams, ctm, font, where, active):
        resources = self.res(resources) or {}
        xobjects = self.res(resources.get('XObject')) or {}
        fonts = self.res(resources.get('Font')) or {}
        states = self.res(resources.get('ExtGState')) or {}
        stack = []
        for stream in streams:
            try:
                operations = self.ops(stream)
            except Unsupported as e:
                self.unsupported(where, e)
                continue
            for op, args in operations:
                if op == b'q':
                    stack.append((ctm, font))
                elif op == b'Q':
                    if stack:
                        ctm, font = stack.pop()
                elif op == b'cm':
                    m = number_matrix(args)
                    if m:
                        ctm = matrix_then(m, ctm)
                elif op == b'Tf' and len(args) == 2 and isinstance(args[0], Name):
                    font = fonts.get(args[0])
                    if font is None:
                        self.issue('font.undefined', f'{where} selects /{args[0]} but the resources have no such font')
                elif op in (b'Tj', b"'", b'"') and args and isinstance(args[-1], bytes):
                    self.show(font, [args[-1]], where)
                elif op == b'TJ' and args and isinstance(args[-1], list):
                    self.show(font, [x for x in args[-1] if isinstance(x, bytes)], where)
                elif op == b'Do' and args and isinstance(args[-1], Name):
                    ref = xobjects.get(args[-1])
                    obj = self.res(ref)
                    if not isinstance(obj, Stream):
                        self.issue('xobject.undefined', f'{where} draws /{args[-1]} but the resources have no such XObject (ISO 32000-1 7.8.3)')
                        continue
                    if obj.get('Subtype') == 'Image':
                        self.place(ref, obj, ctm, where)
                    elif obj.get('Subtype') == 'Form' and id(obj) not in active and len(active) < 12:
                        matrix = number_matrix([self.res(v) for v in (self.res(obj.get('Matrix')) or IDENTITY)]) or IDENTITY
                        self.scan(self.res(obj.get('Resources')) or resources, [obj], matrix_then(matrix, ctm), font, where+' form', active | {id(obj)})
                elif op == b'BI' and args and isinstance(args[0], dict):
                    self.place(None, args[0], ctm, where)
                elif op == b'gs' and args and isinstance(args[-1], Name):
                    # a soft-mask group is drawn in the coordinate system current at `gs` (ISO 32000-1 11.6.5.2)
                    mask = (self.res(states.get(args[-1])) or {}).get('SMask')
                    group = self.res(self.res(mask).get('G')) if isinstance(self.res(mask), dict) else None
                    if isinstance(group, Stream) and id(group) not in active and len(active) < 12:
                        matrix = number_matrix([self.res(v) for v in (self.res(group.get('Matrix')) or IDENTITY)]) or IDENTITY
                        self.scan(self.res(group.get('Resources')) or resources, [group], matrix_then(matrix, ctm), font, where+' soft mask', active | {id(group)})

    def place(self, ref, obj, ctm, where):
        """Effective resolution of one drawn picture: pixels over the drawn size (ISO 32000-1 8.9.3, unit square image space)."""
        if ref is not None:
            self.placed.add(ref)
        width, height = self.res(obj.get('Width')), self.res(obj.get('Height'))
        inline = ref is None
        space = self.res(obj.get('ColorSpace'))
        space = str(space[0]) if isinstance(space, list) and space else str(space) if space is not None else None
        bitmap = bool(self.res(obj.get('ImageMask'))) or self.res(obj.get('BitsPerComponent')) == 1
        drawn = (math.hypot(ctm[0], ctm[1]), math.hypot(ctm[2], ctm[3]))
        row = {'page': self.pages, 'object': None if inline else ref[0], 'inline': inline, 'width': width, 'height': height,
               'drawnPoints': [round(drawn[0], 3), round(drawn[1], 3)], 'space': space, 'bitmap': bitmap, 'ppi': None}
        if isinstance(width, int) and isinstance(height, int) and width > 0 and height > 0 and drawn[0] > 1e-9 and drawn[1] > 1e-9:
            x, y = width / (drawn[0] / 72), height / (drawn[1] / 72)
            row['ppi'] = [round(x, 2), round(y, 2)]
            floor = self.limits.get('bitmap_ppi' if bitmap else 'ppi')
            if floor and min(x, y) < floor:
                self.issue('image.resolution', f'{where} object {row["object"]}: {x:.1f} x {y:.1f} ppi is below {floor:g}', True)
        else:
            self.issue('image.resolution', f'{where} object {row["object"]}: drawn size or pixel size is zero, resolution undefined', True)
        self.images.append(row)

    def show(self, font, strings, where):
        if font is None:
            self.issue('font.undefined', where+' shows text without a font selected by Tf')
            return
        key = font if isinstance(font, Ref) else id(font)
        entry = self.font_uses.setdefault(key, [self.res(font), set()])
        entry[1].update(strings)

    # ---- fonts: program present, readable, and covering the glyphs the text uses --------------------------

    def check_fonts(self):
        for font, strings in self.font_uses.values():
            if not isinstance(font, dict):
                continue
            name = text(font.get('BaseFont')) or 'unnamed font'
            try:
                self.font_coverage(font, strings)
            except Unsupported as e:
                self.unchecked.append({'where': 'font '+name, 'reason': str(e)})
                self.issue('coverage.font', f'font {name}: {e}', True)
            except (IndexError, ValueError, KeyError, TypeError, zlib.error) as e:
                self.issue('font.program', f'{name}: font data could not be read ({type(e).__name__}: {e})')

    def font_coverage(self, font, strings):
        name = text(font.get('BaseFont')) or 'unnamed font'
        info = {'name': name, 'subtype': str(font.get('Subtype')), 'subset': bool(re.match(r'[A-Z]{6}\+', name)),
                'program': None, 'used': 0, 'missing': 0, 'checked': False}
        self.fonts.append(info)

        def skip(reason):
            self.unchecked.append({'where': 'font '+name, 'reason': reason})
            self.issue('coverage.font', f'font {name}: {reason}', True)

        subtype, descendant, to_u = font.get('Subtype'), None, to_unicode(self.res(font.get('ToUnicode')), self.res)
        if subtype == 'Type3':
            return skip('Type3 glyph procedures are not inspected')
        if subtype == 'Type0':
            kids = self.res(font.get('DescendantFonts'))
            descendant = self.res(kids[0]) if isinstance(kids, list) and kids else None
            if not isinstance(descendant, dict):
                return
            if self.res(font.get('Encoding')) not in ('Identity-H', 'Identity-V'):
                return skip(f'CMap {self.res(font.get("Encoding"))} (only Identity-H/V codes are mapped)')
            desc = self.res(descendant.get('FontDescriptor')) or {}
        else:
            desc = self.res(font.get('FontDescriptor')) or {}
        found = [(k, self.res(desc.get(k))) for k in ('FontFile', 'FontFile2', 'FontFile3') if isinstance(self.res(desc.get(k)), Stream)]
        if not found:
            return                      # absent program: font.embedded already reports it
        kind, stream = found[0]
        try:
            data = decoded(stream)
        except Unsupported as e:
            return skip(f'font program stream {e}')
        except (ValueError, zlib.error) as e:
            return self.issue('font.program', f'{name}: font program stream cannot be decoded ({e})')
        if not data:
            return self.issue('font.program', f'{name}: {kind} is empty')
        info['program'] = kind
        if kind == 'FontFile':
            if not (data[:2] == b'%!' or data[:1] == b'\x80'):
                self.issue('font.program', f'{name}: FontFile is not a Type 1 font program')
            return skip('Type 1 font program: glyphs are not checked by name')
        try:
            if kind == 'FontFile3' and self.res(stream.get('Subtype')) in ('Type1C', 'CIDFontType0C'):
                program = CFFOnly(data)
            else:
                program = TrueType(data)
            maps = program.cmap() if subtype != 'Type0' else {}
        except Unsupported as e:
            return skip(str(e))
        except (ValueError, IndexError, KeyError) as e:
            return self.issue('font.program', f'{name}: font program is damaged ({e})')
        info['checked'] = True
        gids, blank_ok = {}, set()
        if subtype == 'Type0':
            cid_map = self.res(descendant.get('CIDToGIDMap'))
            table = decoded(cid_map) if isinstance(cid_map, Stream) else None
            for s in strings:
                if len(s) % 2:
                    self.issue('font.codes', f'{name}: a text string has an odd number of bytes for a two-byte font')
                for i in range(0, len(s) - 1, 2):
                    cid = u16(s, i)
                    if table is not None:
                        gid = u16(table, 2*cid) if 2*cid + 2 <= len(table) else None
                    elif descendant.get('Subtype') == 'CIDFontType0' and program.cff and program.cff.cid:
                        gid = program.cff.charset.get(cid)
                    else:
                        gid = cid
                    gids[cid] = gid
        else:
            if subtype != 'TrueType':
                return skip(f'{subtype} simple font: glyphs are not checked by name')
            encoding = self.res(font.get('Encoding'))
            if isinstance(encoding, dict):
                return skip('TrueType font with an Encoding dictionary')
            symbolic = bool(self.res(desc.get('Flags')) & 4) if isinstance(self.res(desc.get('Flags')), int) else False
            unicode_map, mac_map, symbol_map = maps.get((3, 1)) or maps.get((0, 3)) or maps.get((0, 4)), maps.get((1, 0)), maps.get((3, 0))
            for s in strings:
                for code in s:
                    gid = 0
                    if encoding == 'WinAnsiEncoding' and unicode_map and not symbolic:
                        gid = unicode_map(ord(bytes([code]).decode('cp1252', 'replace')))
                    elif symbol_map:
                        gid = symbol_map(0xF000 + code) or symbol_map(code)
                    elif mac_map:
                        gid = mac_map(code)
                    elif unicode_map:
                        gid = unicode_map(code)
                    gids[code] = gid
                    if code in (32, 160):
                        blank_ok.add(code)
        notdef, missing = [], []
        for code, gid in gids.items():
            shown = to_u.get(code)
            blank_ok_here = code in blank_ok or (shown is not None and shown.strip() == '')
            if gid is None:
                missing.append(code)
            elif gid == 0:
                (notdef if subtype == 'Type0' or blank_ok_here else missing).append(code)
            else:
                outline = program.outline(gid)
                if outline is None:
                    missing.append(code)            # glyph index beyond the font's glyph count
                elif outline is False and not blank_ok_here:
                    if shown is not None or subtype != 'Type0':
                        missing.append(code)        # a character that must be drawn maps to an emptied glyph
                    else:
                        self.issue('font.glyph_blank', f'{name}: glyph {gid} has no outline and no ToUnicode entry says it is a space', True)
        info['used'], info['missing'] = len(gids), len(missing)
        if notdef:
            self.issue('font.notdef', f'{name}: text shows the .notdef glyph for code(s) {sorted(notdef)[:8]}', True)
        if missing:
            self.issue('font.glyph_missing', f'{name}: {len(missing)} used glyph(s) are missing or blank in the {"subset " if info["subset"] else ""}font program, codes {sorted(missing)[:8]}')

    def finish(self):
        for ref in sorted(self.image_refs - self.placed - self.mask_refs):
            self.issue('image.unplaced', f'object {ref[0]} is a picture no inspected content draws; its resolution is unknown', True)
        self.check_fonts()
        self.issue('coverage', 'Structural audit only: ICC colour transforms, picture pixel content and glyph shapes are not certified', True)

    def duplicates(self):
        # A later incremental revision may legally replace an object (ISO 32000-1 7.5.6).
        if len(self.pdf.sections) > 1:
            reason = 'incremental/hybrid xrefs: a later revision may replace objects, so unindexed or duplicate object declarations were not checked'
            self.issue('coverage.revisions', reason, True)
            self.unchecked.append({'where': 'file', 'reason': reason})
            return
        end = 0
        declarations = lambda data: re.search(rb'(?<!\w)\d+\s+\d+\s+obj\b', re.sub(rb'%[^\r\n]*', b'', data))
        for start, stop in sorted(set(self.pdf.extents)):
            if start >= end:
                if declarations(self.data[end:start]):
                    self.issue('object.unindexed', 'unindexed/duplicate object declaration outside streams')
            end = max(end, stop)
        if declarations(self.data[end:min(self.pdf.sections)]):
            self.issue('object.unindexed', 'unindexed/duplicate object declaration after indexed objects')


def verify_bytes(data, limits=None):
    """Result: errors, warnings, and what could not be inspected.

    status: "checked" (everything inspected), "partial" (some parts were skipped, see unchecked) or
    "unchecked" (the file could not be read far enough to audit; the reason is in unchecked).
    """
    audit = Audit(data, limits)
    status = 'checked'
    try:
        audit.run()
    except Unsupported as e:
        audit.issue('coverage.unsupported', e, True)
        audit.unchecked.append({'where': 'file', 'reason': str(e)})
        status = 'unchecked'
    except (ValueError, KeyError, TypeError, AttributeError, IndexError, OverflowError, RecursionError, ET.ParseError, zlib.error) as e:
        audit.issue('structure.parse', e)
    if audit.unchecked and status == 'checked':
        status = 'partial'
    return {'variant': audit.variant, 'pages': audit.pages, 'status': status, 'unchecked': audit.unchecked,
            'images': audit.images, 'fonts': audit.fonts,
            'errors': [x for x in audit.issues if x['level'] == 'error'],
            'warnings': [x for x in audit.issues if x['level'] == 'warning']}


def verify(path, limits=None):
    try:
        return verify_bytes(Path(path).read_bytes(), limits)
    except OSError as e:
        return {'variant': 'unknown', 'pages': 0, 'status': 'unchecked', 'unchecked': [{'where': 'file', 'reason': str(e)}], 'images': [], 'fonts': [],
                'errors': [{'level': 'error', 'code': 'file.read', 'detail': str(e)}], 'warnings': []}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pdf', type=Path)
    parser.add_argument('--json', action='store_true')
    parser.add_argument('--report-only', action='store_true', help='report errors without a failing exit code')
    parser.add_argument('--min-ppi', type=float, help='warn for colour/gray pictures drawn below this effective ppi (ISO 15930 sets none)')
    parser.add_argument('--min-bitmap-ppi', type=float, help='warn for 1-bit pictures and stencil masks below this effective ppi')
    args = parser.parse_args()
    result = verify(args.pdf, {'ppi': args.min_ppi, 'bitmap_ppi': args.min_bitmap_ppi})
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f'{args.pdf.name}: {result["variant"]}, {result["pages"]} pages, {len(result["errors"])} errors, {len(result["warnings"])} warnings, inspection {result["status"]}')
        placed = [i for i in result['images'] if i['ppi']]
        if placed:
            low = min(placed, key=lambda i: min(i['ppi']))
            print(f'images: {len(result["images"])} drawn, lowest effective ppi {min(low["ppi"]):g} (page {low["page"]}, object {low["object"]})')
        for row in result['errors']+result['warnings']:
            print(f'{row["level"]}: {row["code"]}: {row["detail"]}')
        for row in result['unchecked']:
            print(f'not inspected: {row["where"]}: {row["reason"]}')
    return int(bool(result['errors']) and not args.report_only)


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    sys.exit(main())
