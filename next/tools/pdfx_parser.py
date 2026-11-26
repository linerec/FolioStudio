"""Read PDF objects, cross-reference tables, and encoded streams."""
import base64
import re
import zlib


class Unsupported(ValueError):
    pass


class Name(str):
    pass


class Ref(tuple):
    pass


class Stream(dict):
    def __init__(self, values, data):
        super().__init__(values)
        self.data = data


class Reader:
    """PDF values, preserving names, strings and references as distinct types."""
    def __init__(self, data, pos=0):
        self.data, self.pos = data, pos

    def skip(self):
        m = re.compile(rb'(?:[\x00\t\n\f\r ]|%[^\r\n]*)*').match(self.data, self.pos)
        self.pos = m.end()

    def token(self):
        self.skip()
        m = re.compile(rb'[^\x00\t\n\f\r ()<>\[\]{}/%]+').match(self.data, self.pos)
        if not m:
            raise ValueError(f'expected token at {self.pos}')
        self.pos = m.end()
        return m.group()

    def value(self, depth=0):
        if depth > 100:
            raise Unsupported('object nesting exceeds inspection limit')
        self.skip()
        b, p = self.data, self.pos
        if b[p:p+2] == b'<<':
            self.pos += 2
            result = {}
            while True:
                self.skip()
                if b[self.pos:self.pos+2] == b'>>':
                    self.pos += 2
                    return result
                key = self.value(depth+1)
                if not isinstance(key, Name):
                    raise ValueError('dictionary key is not a name')
                if key in result:
                    raise ValueError(f'duplicate dictionary key /{key}')
                result[key] = self.value(depth+1)
        if b[p:p+1] == b'[':
            self.pos += 1
            result = []
            while True:
                self.skip()
                if b[self.pos:self.pos+1] == b']':
                    self.pos += 1
                    return result
                result.append(self.value(depth+1))
        if b[p:p+1] == b'/':
            self.pos += 1
            m = re.compile(rb'[^\x00\t\n\f\r ()<>\[\]{}/%]*').match(b, self.pos)
            self.pos = m.end()
            return Name(re.sub(rb'#([0-9a-fA-F]{2})', lambda x: bytes([int(x[1], 16)]), m.group()).decode('latin1'))
        if b[p:p+1] == b'(':
            return self.string()
        if b[p:p+1] == b'<':
            end = b.index(b'>', p+1)
            h = re.sub(rb'\s', b'', b[p+1:end])
            self.pos = end+1
            return bytes.fromhex((h + (b'0' if len(h) % 2 else b'')).decode('ascii'))
        t = self.token()
        if re.fullmatch(rb'[+-]?\d+', t):
            end = self.pos
            m = re.compile(rb'\s+(\d+)\s+R\b').match(b, end)
            if m:
                self.pos = m.end()
                return Ref((int(t), int(m[1])))
            return int(t)
        if re.fullmatch(rb'[+-]?(?:\d+\.\d*|\.\d+)', t):
            return float(t)
        if t in (b'true', b'false', b'null'):
            return {b'true': True, b'false': False, b'null': None}[t]
        raise ValueError(f'invalid PDF value {t[:40]!r}')

    def string(self):
        self.pos += 1
        out, nesting = bytearray(), 1
        while self.pos < len(self.data):
            c = self.data[self.pos]
            self.pos += 1
            if c == 92:
                c = self.data[self.pos]
                self.pos += 1
                if c in b'01234567':
                    m = re.match(rb'[0-7]{0,2}', self.data[self.pos:])
                    out.append(int(bytes([c])+m.group(), 8) % 256)
                    self.pos += len(m.group())
                elif c in (10, 13):
                    if c == 13 and self.data[self.pos:self.pos+1] == b'\n':
                        self.pos += 1
                else:
                    out.append({110:10, 114:13, 116:9, 98:8, 102:12}.get(c, c))
                continue
            if c == 40:
                nesting += 1
            elif c == 41:
                nesting -= 1
                if not nesting:
                    return bytes(out)
            out.append(c)
        raise ValueError('unterminated literal string')


def lzw_decode(data, early=1):
    """LZWDecode (ISO 32000-1 7.4.4): MSB-first codes of 9..12 bits, 256 = clear, 257 = end."""
    out, table, width, prev, acc, nbits = bytearray(), [bytes([i]) for i in range(256)] + [b'', b''], 9, None, 0, 0
    for byte in data:
        acc, nbits = (acc << 8) | byte, nbits + 8
        while nbits >= width:
            code = (acc >> (nbits - width)) & ((1 << width) - 1)
            nbits -= width
            acc &= (1 << nbits) - 1
            if code == 256:
                table, width, prev = table[:258], 9, None
                continue
            if code == 257:
                return bytes(out)
            if prev is None:
                if code >= len(table):
                    raise ValueError('invalid LZW stream')
                entry = table[code]
            else:
                if code > len(table):
                    raise ValueError('invalid LZW stream')
                entry = table[code] if code < len(table) else prev + prev[:1]
                table.append(prev + entry[:1])
            out += entry
            prev = entry
            size = len(table) + early
            width = 9 if size < 512 else 10 if size < 1024 else 11 if size < 2048 else 12
    return bytes(out)


def undo_predictor(data, parms):
    predictor = parms.get('Predictor', 1)
    if predictor == 1:
        return data
    colors, bpc, columns = parms.get('Colors', 1), parms.get('BitsPerComponent', 8), parms.get('Columns', 1)
    if not all(isinstance(v, int) and v > 0 for v in (colors, bpc, columns)):
        raise ValueError('invalid predictor parameters')
    row = (colors * bpc * columns + 7) // 8
    bpp = max(1, colors * bpc // 8)
    if predictor == 2:
        if bpc != 8:
            raise Unsupported('TIFF predictor with other than 8 bits per component (content not inspected)')
        out = bytearray(data)
        for start in range(0, len(out) - row + 1, row):
            for i in range(bpp, row):
                out[start+i] = (out[start+i] + out[start+i-bpp]) & 255
        return bytes(out)
    if predictor < 10 or predictor > 15:
        raise Unsupported(f'stream predictor {predictor} (content not inspected)')
    out, prior = bytearray(), bytearray(row)
    for start in range(0, len(data), row+1):
        tag, line = data[start], bytearray(data[start+1:start+1+row])
        line += bytes(row - len(line))
        for i in range(row):
            left = line[i-bpp] if i >= bpp else 0
            up, upleft = prior[i], (prior[i-bpp] if i >= bpp else 0)
            if tag == 1:
                line[i] = (line[i] + left) & 255
            elif tag == 2:
                line[i] = (line[i] + up) & 255
            elif tag == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif tag == 4:
                p = left + up - upleft
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - upleft)
                line[i] = (line[i] + (left if pa <= pb and pa <= pc else up if pb <= pc else upleft)) & 255
            elif tag != 0:
                raise ValueError('invalid PNG predictor row tag')
        out += line
        prior = line
    return bytes(out)


def decoded(stream):
    if not isinstance(stream, Stream):
        raise ValueError('expected stream')
    data = stream.data
    filters = stream.get('Filter', [])
    filters = filters if isinstance(filters, list) else [filters]
    parms = stream.get('DecodeParms', [{}] * len(filters))
    parms = parms if isinstance(parms, list) else [parms]
    for i, f in enumerate(filters):
        if f in ('FlateDecode', 'Fl'):
            data = zlib.decompress(data)
        elif f in ('LZWDecode', 'LZW'):
            p = parms[i] if i < len(parms) and isinstance(parms[i], dict) else {}
            data = lzw_decode(data, p.get('EarlyChange', 1))
        elif f in ('ASCIIHexDecode', 'AHx'):
            h = re.sub(rb'\s', b'', data).rstrip(b'>')
            data = bytes.fromhex((h + (b'0' if len(h) % 2 else b'')).decode())
        elif f in ('ASCII85Decode', 'A85'):
            data = base64.a85decode(data.rstrip().removesuffix(b'~>'), adobe=False)
        else:
            raise Unsupported(f'stream filter {f}')
        if f in ('FlateDecode', 'Fl', 'LZWDecode', 'LZW') and i < len(parms) and isinstance(parms[i], dict) and parms[i]:
            data = undo_predictor(data, parms[i])
    return data


class PDF:
    def __init__(self, data):
        self.data, self.entries, self.objects = data, {}, {}
        self.loading, self.sections, self.extents = set(), set(), []
        tail = re.search(rb'startxref\s+(\d+)\s+%%EOF\s*$', data)
        if not tail:
            raise ValueError('missing final startxref / %%EOF')
        self.trailer = self.xref(int(tail[1]))
        for num, entry in self.entries.items():
            if entry[0] in (1, 2):
                self.resolve(Ref((num, entry[2] if entry[0] == 1 else 0)))

    def indirect(self, offset):
        if not re.match(rb'\d+\s+\d+\s+obj\b', self.data[offset:offset+80]):
            raise ValueError(f'xref offset {offset} does not start an object header')
        r = Reader(self.data, offset)
        number, generation = int(r.token()), int(r.token())
        if r.token() != b'obj':
            raise ValueError(f'xref offset {offset} does not address an object')
        value = r.value()
        r.skip()
        if self.data[r.pos:r.pos+6] == b'stream':
            m = re.match(rb'stream(?:\r\n|\n|\r)', self.data[r.pos:])
            if not m or not isinstance(value, dict):
                raise ValueError('invalid stream header')
            start = r.pos + len(m.group())
            length = self.resolve(value.get('Length'))
            if not isinstance(length, int) or length < 0:
                raise ValueError('invalid stream Length')
            r.pos = start+length
            if r.token() != b'endstream':
                raise ValueError('stream Length does not end at endstream')
            value = Stream(value, self.data[start:start+length])
        if r.token() != b'endobj':
            raise ValueError('missing endobj')
        self.extents.append((offset, r.pos))
        return Ref((number, generation)), value

    def xref(self, offset):
        if offset in self.sections:
            raise ValueError('cyclic xref chain')
        self.sections.add(offset)
        r = Reader(self.data, offset)
        section_entries = set()
        if self.data[offset:offset+4] == b'xref':
            r.token()
            while True:
                t = r.token()
                if t == b'trailer':
                    break
                first, count = int(t), int(r.token())
                for num in range(first, first+count):
                    if num in section_entries:
                        raise ValueError('duplicate object number in xref section')
                    section_entries.add(num)
                    off, gen, state = int(r.token()), int(r.token()), r.token()
                    if state not in (b'n', b'f'):
                        raise ValueError('invalid xref entry')
                    self.entries.setdefault(num, (int(state == b'n'), off, gen))
            trailer = r.value()
        else:
            _, trailer = self.indirect(offset)
            if trailer.get('Type') != 'XRef':
                raise ValueError('startxref does not address xref')
            data, widths = decoded(trailer), trailer['W']
            if len(widths) != 3 or any(not isinstance(w, int) or w < 0 for w in widths):
                raise ValueError('invalid xref field widths')
            ranges, pos = trailer.get('Index', [0, trailer['Size']]), 0
            for first, count in zip(ranges[::2], ranges[1::2]):
                for num in range(first, first+count):
                    if num in section_entries:
                        raise ValueError('duplicate object number in xref stream')
                    section_entries.add(num)
                    fields = []
                    for width in widths:
                        if pos+width > len(data):
                            raise ValueError('truncated xref stream')
                        fields.append(int.from_bytes(data[pos:pos+width], 'big'))
                        pos += width
                    if widths[0] == 0:
                        fields[0] = 1
                    self.entries.setdefault(num, tuple(fields))
        if 'XRefStm' in trailer:
            self.xref(trailer['XRefStm'])
        if 'Prev' in trailer:
            previous = self.xref(trailer['Prev'])
            trailer = dict(previous, **trailer)
        return trailer

    def resolve(self, value):
        if not isinstance(value, Ref):
            return value
        if value in self.objects:
            return self.objects[value]
        if value in self.loading:
            raise ValueError(f'cyclic object indirection {value}')
        self.loading.add(value)
        entry = self.entries.get(value[0])
        if not entry or not entry[0]:
            raise ValueError(f'dangling reference {value}')
        kind, offset, gen = entry
        if kind == 1:
            key, obj = self.indirect(offset)
            if key != value or gen != value[1]:
                raise ValueError(f'xref object identity mismatch {value}')
        elif kind == 2:
            container = self.resolve(Ref((offset, 0)))
            data = decoded(container)
            header = Reader(data)
            indices = [(int(header.token()), int(header.token())) for _ in range(container['N'])]
            if value[1] != 0 or gen >= len(indices) or indices[gen][0] != value[0]:
                raise ValueError('object stream index mismatch')
            obj = Reader(data, container['First']+indices[gen][1]).value()
        else:
            raise Unsupported(f'xref entry type {kind}')
        self.objects[value] = obj
        self.loading.remove(value)
        return obj
