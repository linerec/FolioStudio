"""Ink usage of a PDF read from its content streams: what colour every glyph and every painted
object uses, and which paint operations overprint.

Usage: python inkcheck.py file.pdf [out.json]

The graphics state is followed through q/Q, gs (OP/op/OPM), colour operators (k/K, g/G,
rg/RG, cs/CS + sc/scn), text (Tj/TJ/'/", glyph count from the font's code width) and form
XObjects (recursively, inheriting the state). Counts:
  glyphs by fill colour: DeviceCMYK K-only (black-tint text), other DeviceCMYK, per colour
  space; K-only tints histogram; glyphs drawn with fill overprint;
  fills / strokes / shadings with overprint on, by colour class;
  inks named by Separation / DeviceN spaces that were painted.
"""
import sys, json, re, collections
sys.stdout.reconfigure(encoding='utf-8')
from pypdf import PdfReader
from pypdf.generic import IndirectObject, ArrayObject, NameObject, DictionaryObject, StreamObject


def res(o):
    return o.get_object() if isinstance(o, IndirectObject) else o


TOKEN = re.compile(rb'''
    (?P<ws>\s+|%[^\r\n]*)
  | (?P<str>\()
  | (?P<hex><[0-9A-Fa-f\s]*>)
  | (?P<dict><<|>>)
  | (?P<arr>[\[\]])
  | (?P<name>/[^\s/\[\]()<>{}%]*)
  | (?P<num>[+-]?(?:\d+\.?\d*|\.\d+))
  | (?P<op>[^\s/\[\]()<>{}%]+)
''', re.X)


def tokens(data):
    i, n = 0, len(data)
    while i < n:
        m = TOKEN.match(data, i)
        if not m:
            i += 1
            continue
        k = m.lastgroup
        if k == 'ws':
            i = m.end()
            continue
        if k == 'str':
            depth, j, out = 1, m.end(), bytearray()
            while j < n and depth:
                c = data[j]
                if c == 0x5C:
                    out += data[j:j + 2]
                    j += 2
                    continue
                if c == 0x28:
                    depth += 1
                elif c == 0x29:
                    depth -= 1
                    if depth == 0:
                        j += 1
                        break
                out.append(c)
                j += 1
            yield ('str', unescape(bytes(out)))
            i = j
            continue
        if k == 'hex':
            h = re.sub(rb'\s', b'', m.group()[1:-1])
            if len(h) % 2:
                h += b'0'
            yield ('str', bytes.fromhex(h.decode()))
            i = m.end()
            continue
        if k == 'op' and m.group() == b'ID':
            # inline image data up to EI
            e = data.find(b'EI', m.end())
            yield ('op', b'ID')
            i = (e + 2) if e >= 0 else n
            continue
        yield (k, m.group())
        i = m.end()


def unescape(b):
    out, i = bytearray(), 0
    while i < len(b):
        c = b[i]
        if c == 0x5C and i + 1 < len(b):
            d = b[i + 1]
            if d in b'01234567':
                j = i + 1
                v = 0
                while j < len(b) and j < i + 4 and b[j] in b'01234567':
                    v = v * 8 + (b[j] - 48)
                    j += 1
                out.append(v & 0xFF)
                i = j
                continue
            out.append({ord('n'): 10, ord('r'): 13, ord('t'): 9, ord('b'): 8, ord('f'): 12}.get(d, d))
            i += 2
            continue
        out.append(c)
        i += 1
    return bytes(out)


class State:
    def __init__(self):
        self.fill = ('DeviceGray', (0.0,))
        self.stroke = ('DeviceGray', (0.0,))
        self.op = False
        self.OP = False
        self.font_bytes = 1
        self.tr = 0

    def copy(self):
        s = State()
        s.__dict__.update(self.__dict__)
        return s


def cs_kind(cs):
    cs = res(cs)
    if isinstance(cs, NameObject):
        return str(cs)[1:], None
    if isinstance(cs, ArrayObject):
        k = str(cs[0])[1:]
        if k == 'Separation':
            return 'Separation', [str(cs[1])[1:]]
        if k == 'DeviceN':
            return 'DeviceN', [str(x)[1:] for x in res(cs[1])]
        if k == 'ICCBased':
            n = res(cs[1]).get('/N')
            return f'ICCBased{n}', None
        return k, None
    return 'unknown', None


def colour_class(c):
    space, v = c
    if space == 'DeviceCMYK' and len(v) == 4:
        cc, m, y, k = v
        if cc == 0 and m == 0 and y == 0:
            return 'CMYK K-only' if k > 0 else 'CMYK white'
        return 'CMYK'
    if space.startswith('Separation:'):
        return 'Separation ' + space.split(':', 1)[1]
    return space.split(':')[0]


class Walker:
    def __init__(self, reader):
        self.r = reader
        self.glyphs = collections.Counter()
        self.ktints = collections.Counter()
        self.op_glyphs = collections.Counter()
        self.op_fills = collections.Counter()
        self.op_strokes = collections.Counter()
        self.fills = collections.Counter()
        self.strokes = collections.Counter()
        self.inks = collections.Counter()
        self.forms_seen = 0
        self.page = 0
        # K-only strokes / fills / glyphs per page (1-based), to locate differences
        self.k_by_page = collections.defaultdict(collections.Counter)

    def font_width(self, fonts, name):
        f = res(fonts.get(name)) if fonts else None
        if f is None:
            return 1
        return 2 if f.get('/Subtype') == '/Type0' else 1

    def run(self, data, rd, st, depth=0):
        rd = res(rd) or DictionaryObject()
        cspaces = res(rd.get('/ColorSpace')) or {}
        gstates = res(rd.get('/ExtGState')) or {}
        fonts = res(rd.get('/Font')) or {}
        xobjs = res(rd.get('/XObject')) or {}
        stack = []
        ops = []
        for kind, val in tokens(data):
            if kind != 'op':
                ops.append((kind, val))
                continue
            op = val.decode('latin-1')
            nums = [float(v) for k, v in ops if k == 'num']
            names = [v.decode('latin-1') for k, v in ops if k == 'name']
            strs = [v for k, v in ops if k == 'str']
            ops = []
            if op == 'q':
                stack.append(st.copy())
            elif op == 'Q':
                if stack:
                    st = stack.pop()
            elif op == 'gs' and names:
                g = res(gstates.get(names[-1]))
                if g is not None:
                    # pypdf BooleanObject is truthy whatever its value: compare instead
                    if '/OP' in g:
                        st.OP = (g['/OP'] == True)
                        if '/op' not in g:
                            st.op = st.OP
                    if '/op' in g:
                        st.op = (g['/op'] == True)
            elif op in ('k', 'K'):
                c = ('DeviceCMYK', tuple(round(x, 4) for x in nums[-4:]))
                if op == 'k':
                    st.fill = c
                else:
                    st.stroke = c
            elif op in ('g', 'G'):
                c = ('DeviceGray', tuple(nums[-1:]))
                if op == 'g':
                    st.fill = c
                else:
                    st.stroke = c
            elif op in ('rg', 'RG'):
                c = ('DeviceRGB', tuple(nums[-3:]))
                if op == 'rg':
                    st.fill = c
                else:
                    st.stroke = c
            elif op in ('cs', 'CS') and names:
                n = names[-1]
                if n in ('/DeviceCMYK', '/DeviceRGB', '/DeviceGray', '/Pattern'):
                    k, inks = n[1:], None
                else:
                    k, inks = cs_kind(cspaces.get(n))
                if inks:
                    k = f'{k}:{",".join(inks)}'
                if op == 'cs':
                    st.fill = (k, ())
                else:
                    st.stroke = (k, ())
            elif op in ('sc', 'scn', 'SC', 'SCN'):
                if op.islower():
                    st.fill = (st.fill[0], tuple(round(x, 4) for x in nums))
                else:
                    st.stroke = (st.stroke[0], tuple(round(x, 4) for x in nums))
            elif op == 'Tf' and names:
                st.font_bytes = self.font_width(fonts, names[-1])
            elif op == 'Tr' and nums:
                st.tr = int(nums[-1])
            elif op in ('Tj', "'", '"', 'TJ'):
                n = sum(len(s) for s in strs) // max(1, st.font_bytes)
                if st.tr in (3, 7):
                    continue
                cls = colour_class(st.fill)
                self.glyphs[cls] += n
                if cls == 'CMYK K-only':
                    self.ktints[st.fill[1][3]] += n
                if st.op:
                    self.op_glyphs[cls] += n
                self.note_inks(st.fill)
            elif op in ('f', 'F', 'f*', 'B', 'B*', 'b', 'b*'):
                cls = colour_class(st.fill)
                self.fills[cls] += 1
                if st.op:
                    self.op_fills[cls] += 1
                self.note_inks(st.fill)
                if op.lower().startswith('b'):
                    self.stroke_op(st)
            elif op in ('S', 's'):
                self.stroke_op(st)
            elif op == 'sh':
                self.fills['shading'] += 1
                if st.op:
                    self.op_fills['shading'] += 1
            elif op == 'Do' and names:
                x = res(xobjs.get(names[-1]))
                if isinstance(x, StreamObject) and x.get('/Subtype') == '/Form' and depth < 32:
                    self.forms_seen += 1
                    self.run(x.get_data(), x.get('/Resources') or rd, st.copy(), depth + 1)
                elif isinstance(x, StreamObject) and x.get('/Subtype') == '/Image':
                    self.fills['image'] += 1
                    if st.op:
                        self.op_fills['image'] += 1

    def stroke_op(self, st):
        cls = colour_class(st.stroke)
        self.strokes[cls] += 1
        if cls == 'CMYK K-only':
            self.k_by_page[self.page]['strokes'] += 1
        if st.OP:
            self.op_strokes[cls] += 1
        self.note_inks(st.stroke)

    def note_inks(self, c):
        space = c[0]
        if ':' in space:
            for ink in space.split(':', 1)[1].split(','):
                self.inks[ink] += 1


def main(path):
    r = PdfReader(path)
    w = Walker(r)
    for i, p in enumerate(r.pages):
        w.page = i + 1
        c = p.get_contents()
        if c is None:
            continue
        w.run(c.get_data(), p.get('/Resources'), State())
    out = {
        'file': path,
        'glyphs': dict(w.glyphs.most_common()),
        'glyphsTotal': sum(w.glyphs.values()),
        'kOnlyTints': {str(k): v for k, v in sorted(w.ktints.items())},
        'overprintGlyphs': dict(w.op_glyphs.most_common()),
        'fills': dict(w.fills.most_common()),
        'overprintFills': dict(w.op_fills.most_common()),
        'strokes': dict(w.strokes.most_common()),
        'overprintStrokes': dict(w.op_strokes.most_common()),
        'inksPainted': dict(w.inks.most_common()),
        'formsEntered': w.forms_seen,
        'kOnlyStrokesByPage': {k: v['strokes'] for k, v in sorted(w.k_by_page.items()) if v['strokes']},
    }
    return out


if __name__ == '__main__':
    o = main(sys.argv[1])
    if len(sys.argv) > 2:
        open(sys.argv[2], 'w', encoding='utf-8').write(json.dumps(o, indent=1, ensure_ascii=False))
    print(json.dumps(o, ensure_ascii=False)[:2500])
