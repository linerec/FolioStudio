"""Writes next/engine/crates/edit/data/unihan-rs.bin: the order of every Han character by Unihan radical and stroke count,
which the index sort order (`edit/src/indexes/han.rs`) uses to put the characters of the extension blocks among the
characters of the main block (InDesign sorts all Han that way, measured: index-c5-han-a, index-c6-han-*).

    python -X utf8 -B next/tools/gen-unihan-rs.py <Unihan.zip | Unihan_IRGSources.txt>

Source: the Unicode Character Database, file Unihan.zip (https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip), field
`kRSUnicode` ("radical.additional strokes", e.g. `85.7`; a simplified radical has a prime or two, `120'.3`; the first value counts).
Download it into a session temporary folder, NOT into the repository. The data is under the Unicode License v3, see
`next/engine/crates/edit/data/UNICODE-LICENSE`. Run again when the Unicode version should move on.

Order (the CLDR "radical-stroke" order): radical number, plain radical before its simplified forms, additional strokes, code
point. Characters with a canonical decomposition (the compatibility ideographs F900..FA6D, FA70..FAD9, 2F800..2FA1D) are left
out: normalisation turns them into the unified ideograph before any comparison. Format (little endian):
    4 bytes "UHRS", u32 count N, then N code points, 3 bytes each, in that order (rank = position).
"""
import os
import re
import struct
import sys
import unicodedata
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, '..', 'engine', 'crates', 'edit', 'data', 'unihan-rs.bin'))


def lines_of(path):
    if path.lower().endswith('.zip'):
        with zipfile.ZipFile(path) as z:
            return z.read('Unihan_IRGSources.txt').decode('utf-8').splitlines()
    with open(path, encoding='utf-8') as f:
        return f.read().splitlines()


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    keyed = []
    for line in lines_of(sys.argv[1]):
        if line.startswith('#') or not line.strip():
            continue
        cp, field, value = line.split('\t')
        if field != 'kRSUnicode':
            continue
        code = int(cp[2:], 16)
        if unicodedata.decomposition(chr(code)) and not unicodedata.decomposition(chr(code)).startswith('<'):
            continue
        m = re.fullmatch(r"(\d+)('*)\.(-?\d+)", value.split(' ')[0])
        assert m, (cp, value)
        keyed.append((int(m.group(1)), len(m.group(2)), int(m.group(3)), code))
    keyed.sort()
    assert len({k[3] for k in keyed}) == len(keyed) and all(k[3] < 1 << 24 for k in keyed)
    out = bytearray(b'UHRS' + struct.pack('<I', len(keyed)))
    for k in keyed:
        out += struct.pack('<I', k[3])[:3]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, 'wb') as f:
        f.write(out)
    print(f'{len(keyed)} characters, {len(out)} bytes -> {OUT}')


if __name__ == '__main__':
    main()
