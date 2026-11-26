"""Convert the original Moby mhyph.txt into Folio's sorted, front-coded dictionary.

No third-party modules. Input is bytes: 0xA5 marks a boundary, CRLF ends a row.
Only ASCII alphabetic words are retained. Conflicting case-folded entries are
omitted, so the caller can use its existing pattern fallback.

Format: b'FMHY1' + little-endian u32 count, then an MSB-first bit stream.
Each record: 4-bit common-prefix length (15 adds an 8-bit extension), followed
by 6-bit suffix symbols and a zero terminator. Symbols 1..26 are a..z; bit 5
marks a boundary after that letter. Prefixes compare these marked symbols.
Records are sorted by the unmarked lowercase word. Last byte is zero-padded.
"""

import argparse
import hashlib
from pathlib import Path
import struct


def entries(source):
    words, conflicts = {}, set()
    excluded = 0
    for row in source.splitlines():
        plain = row.replace(b'\xa5', b'').lower()
        if not plain or any(c < 97 or c > 122 for c in plain) or row.startswith(b'\xa5') or row.endswith(b'\xa5'):
            excluded += 1
            continue
        symbols = []
        for c in row.lower():
            if c == 165:
                if not symbols:
                    raise ValueError('boundary before first letter')
                symbols[-1] |= 32
            else:
                symbols.append(c - 96)
        if symbols[-1] & 32:
            raise ValueError('boundary after last letter')
        marked = bytes(symbols)
        if plain in words and words[plain] != marked:
            conflicts.add(plain)
        words[plain] = marked
    return [(word, words[word]) for word in sorted(words) if word not in conflicts], excluded, len(conflicts)


def encode(records):
    out = bytearray(b'FMHY1' + struct.pack('<I', len(records)))
    buffer = bits = 0

    def put(value, width):
        nonlocal buffer, bits
        if value >= 1 << width:
            raise ValueError('value exceeds field width')
        buffer = (buffer << width) | value
        bits += width
        while bits >= 8:
            bits -= 8
            out.append((buffer >> bits) & 255)
        buffer &= (1 << bits) - 1

    previous = b''
    for _, marked in records:
        prefix = 0
        while prefix < min(len(previous), len(marked)) and previous[prefix] == marked[prefix]:
            prefix += 1
        put(min(prefix, 15), 4)
        if prefix >= 15:
            put(prefix - 15, 8)
        for symbol in marked[prefix:]:
            put(symbol, 6)
        put(0, 6)
        previous = marked
    if bits:
        out.append(buffer << (8 - bits))
    return bytes(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--check', action='store_true', help='compare with existing output without writing')
    args = parser.parse_args()
    source = args.source.read_bytes()
    records, excluded, conflicts = entries(source)
    result = encode(records)
    if len(result) > 700_000:
        raise ValueError('dictionary exceeds 0.7 MB budget')
    if args.check:
        if args.output.read_bytes() != result:
            raise ValueError('generated dictionary differs')
    else:
        args.output.write_bytes(result)
    print(f'input SHA-256: {hashlib.sha256(source).hexdigest()}')
    print(f'rows: {len(source.splitlines())}; words: {len(records)}; excluded rows: {excluded}; conflicting words: {conflicts}')
    print(f'output: {len(result)} bytes; SHA-256: {hashlib.sha256(result).hexdigest()}')


if __name__ == '__main__':
    main()
