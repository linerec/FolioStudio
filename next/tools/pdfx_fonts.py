"""Read embedded TrueType, CFF, and ToUnicode glyph data for the PDF audit."""
import re

from .pdfx_parser import Stream, Unsupported, decoded


def u16(b, p):
    return int.from_bytes(b[p:p+2], 'big')


def u32(b, p):
    return int.from_bytes(b[p:p+4], 'big')


class TrueType:
    """sfnt tables needed to ask "does this glyph exist and have an outline?" (ISO 32000-1 9.9; OpenType)."""
    def __init__(self, data):
        if data[:4] not in (b'\x00\x01\x00\x00', b'true', b'OTTO', b'ttcf'):
            raise ValueError('not an sfnt font program')
        if data[:4] == b'ttcf':
            raise Unsupported('TrueType collection as an embedded font program')
        self.data, self.tables = data, {}
        for i in range(u16(data, 4)):
            tag, off, length = data[12+16*i:16+16*i], u32(data, 20+16*i), u32(data, 24+16*i)
            if off + length > len(data):
                raise ValueError(f'font table {tag.decode("latin1")} extends beyond the font program')
            self.tables[tag.decode('latin1')] = (off, length)
        self.cff = None
        if 'CFF ' in self.tables:
            off, length = self.tables['CFF ']
            self.cff = CFF(data[off:off+length])
            self.glyphs = self.cff.count
            return
        for need in ('maxp', 'head', 'loca', 'glyf'):
            if need not in self.tables:
                raise ValueError(f'font program has no {need} table')
        self.glyphs = u16(data, self.tables['maxp'][0] + 4)
        self.long_loca = u16(data, self.tables['head'][0] + 50)
        count = self.tables['loca'][1] // (4 if self.long_loca else 2)
        if count < self.glyphs + 1:
            raise ValueError('loca table is shorter than numGlyphs')

    def outline(self, gid):
        """True when the glyph has contour data (an empty slot is a blank glyph such as a space)."""
        if gid < 0 or gid >= self.glyphs:
            return None
        if self.cff:
            return self.cff.outline(gid)
        off = self.tables['loca'][0]
        a, b = ((u32(self.data, off+4*gid), u32(self.data, off+4*gid+4)) if self.long_loca
                else (2*u16(self.data, off+2*gid), 2*u16(self.data, off+2*gid+2)))
        return b > a

    def cmap(self):
        """{(platform, encoding): lookup(code) -> gid}; formats 0, 4, 6, 12 (others are skipped)."""
        found = {}
        if 'cmap' not in self.tables:
            return found
        base, d = self.tables['cmap'][0], self.data
        for i in range(u16(d, base + 2)):
            plat, enc, off = u16(d, base+4+8*i), u16(d, base+6+8*i), u32(d, base+8+8*i)
            sub, fmt = base + off, u16(d, base + off)
            if fmt == 0:
                found[(plat, enc)] = lambda c, sub=sub: d[sub+6+c] if 0 <= c < 256 else 0
            elif fmt == 6:
                first, count = u16(d, sub+6), u16(d, sub+8)
                found[(plat, enc)] = lambda c, sub=sub, first=first, count=count: u16(d, sub+10+2*(c-first)) if first <= c < first+count else 0
            elif fmt == 4:
                segs = u16(d, sub+6) // 2
                end, start = sub+14, sub+16+2*segs
                delta, rng = start+2*segs, start+4*segs

                def f4(c, segs=segs, end=end, start=start, delta=delta, rng=rng):
                    for s in range(segs):
                        if c <= u16(d, end+2*s):
                            lo = u16(d, start+2*s)
                            if c < lo:
                                return 0
                            ro = u16(d, rng+2*s)
                            if ro == 0:
                                return (c + u16(d, delta+2*s)) & 0xFFFF
                            g = u16(d, rng+2*s+ro+2*(c-lo))
                            return (g + u16(d, delta+2*s)) & 0xFFFF if g else 0
                    return 0
                found[(plat, enc)] = f4
            elif fmt == 12:
                groups = [(u32(d, sub+16+12*g), u32(d, sub+20+12*g), u32(d, sub+24+12*g)) for g in range(u32(d, sub+12))]
                found[(plat, enc)] = lambda c, groups=groups: next((g0 + c - a for a, z, g0 in groups if a <= c <= z), 0)
        return found


class CFF:
    """Compact Font Format: glyph count, charstring lengths and the CID charset (Adobe TN 5176)."""
    def __init__(self, data):
        if len(data) < 4 or data[0] != 1:
            raise ValueError('not a CFF font program')
        self.data = data
        pos = data[2]
        _, pos = self.index(pos)           # Name INDEX
        tops, pos = self.index(pos)        # Top DICT INDEX
        top = self.dict(tops[0]) if tops else {}
        if 17 not in top:
            raise ValueError('CFF font has no CharStrings')
        items, _ = self.index(top[17][0])
        self.strings, self.count = items, len(items)
        self.cid = (12, 30) in top
        self.charset = {}
        if self.cid:
            off = top.get(15, [0])[0]
            if off > 2:
                self.charset = self.read_charset(off)
            else:
                raise Unsupported('CID-keyed CFF with a predefined charset')

    def index(self, pos):
        d = self.data
        count = u16(d, pos)
        if count == 0:
            return [], pos + 2
        size = d[pos+2]
        if not 1 <= size <= 4:
            raise ValueError('invalid CFF INDEX offSize')
        offs = [int.from_bytes(d[pos+3+size*i:pos+3+size*(i+1)], 'big') for i in range(count+1)]
        base = pos + 3 + size*(count+1) - 1
        if base + offs[-1] > len(d):
            raise ValueError('CFF INDEX extends beyond the font program')
        return [(base+offs[i], base+offs[i+1]) for i in range(count)], base + offs[-1]

    def dict(self, span):
        d, p, out, ops = self.data, span[0], {}, []
        while p < span[1]:
            b = d[p]
            if b <= 21:
                key = (12, d[p+1]) if b == 12 else b
                p += 2 if b == 12 else 1
                out[key] = ops
                ops = []
            elif b == 28:
                ops.append(int.from_bytes(d[p+1:p+3], 'big', signed=True)); p += 3
            elif b == 29:
                ops.append(int.from_bytes(d[p+1:p+5], 'big', signed=True)); p += 5
            elif b == 30:
                p += 1
                while p < span[1] and not (d[p] & 0x0F == 0x0F or d[p] >> 4 == 0x0F):
                    p += 1
                p += 1
                ops.append(0)
            elif 32 <= b <= 246:
                ops.append(b - 139); p += 1
            elif 247 <= b <= 250:
                ops.append((b - 247) * 256 + d[p+1] + 108); p += 2
            elif 251 <= b <= 254:
                ops.append(-(b - 251) * 256 - d[p+1] - 108); p += 2
            else:
                p += 1
        return out

    def read_charset(self, off):
        d, fmt, gid, out = self.data, self.data[off], 1, {}
        p = off + 1
        while gid < self.count:
            if fmt == 0:
                out[u16(d, p)] = gid; p += 2; gid += 1
            else:
                first, left = u16(d, p), (d[p+2] if fmt == 1 else u16(d, p+2))
                p += 3 if fmt == 1 else 4
                for k in range(left + 1):
                    if gid < self.count:
                        out[first+k] = gid; gid += 1
        out[0] = 0
        return out

    def outline(self, gid):
        a, b = self.strings[gid]
        return b - a > 1       # a charstring of only `endchar` is a blank glyph


class CFFOnly:
    """A bare CFF font program (FontFile3 Type1C / CIDFontType0C)."""
    def __init__(self, data):
        self.cff = CFF(data)
        self.glyphs = self.cff.count

    def outline(self, gid):
        return self.cff.outline(gid) if 0 <= gid < self.glyphs else None

    def cmap(self):
        return {}


def to_unicode(stream, res):
    """code -> text from a ToUnicode CMap (bfchar / bfrange)."""
    out = {}
    if not isinstance(stream, Stream):
        return out
    cmap = decoded(stream)
    hexes = lambda s: [bytes.fromhex(x.decode()) for x in re.findall(rb'<([0-9A-Fa-f\s]*)>', s)]
    for block in re.findall(rb'beginbfchar(.*?)endbfchar', cmap, re.S):
        h = hexes(block)
        for src, dst in zip(h[::2], h[1::2]):
            out[int.from_bytes(src, 'big')] = dst.decode('utf-16-be', 'replace')
    for block in re.findall(rb'beginbfrange(.*?)endbfrange', cmap, re.S):
        for lo, hi, rest in re.findall(rb'<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*(<[0-9A-Fa-f]+>|\[[^\]]*\])', block):
            lo, hi = int(lo, 16), int(hi, 16)
            targets = hexes(rest)
            for n in range(lo, min(hi, lo + 65535) + 1):
                if rest.startswith(b'['):
                    if n - lo < len(targets):
                        out[n] = targets[n-lo].decode('utf-16-be', 'replace')
                elif targets:
                    base = int.from_bytes(targets[0], 'big') + n - lo
                    out[n] = base.to_bytes(len(targets[0]), 'big').decode('utf-16-be', 'replace')
    return out
