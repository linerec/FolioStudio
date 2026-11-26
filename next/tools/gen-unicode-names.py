"""Writes next/engine/crates/idml/src/unicode_names.bin: the Unicode character names GREP's `\\N{name}` looks up.

    python -X utf8 -B next/tools/gen-unicode-names.py

Source: this Python's `unicodedata` (Unicode data files, free to use). Names that follow a rule (Hangul syllables, CJK ideographs)
are not stored: `grep_engine::character_named` works them out. Format (little endian):
    u32 word count, then each word as u8 length + bytes;
    u32 name count, then per name: u32 code point, u8 word count, u16 word index per word.
Run again when the Unicode version should move on; the Rust test `names_match_unicode_data` pins a few names.
"""
import os
import struct
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, '..', 'engine', 'crates', 'idml', 'src', 'unicode_names.bin'))
RULE_PREFIXES = ('CJK UNIFIED IDEOGRAPH-', 'CJK COMPATIBILITY IDEOGRAPH-', 'HANGUL SYLLABLE ', 'TANGUT IDEOGRAPH-', 'NUSHU CHARACTER-',
                 'KHITAN SMALL SCRIPT CHARACTER-', 'TANGUT COMPONENT-', 'EGYPTIAN HIEROGLYPH-')


def main():
    names = []
    for cp in range(0x110000):
        if 0xD800 <= cp <= 0xDFFF:
            continue
        try:
            n = unicodedata.name(chr(cp))
        except ValueError:
            continue
        if n.startswith(RULE_PREFIXES):
            continue
        names.append((cp, n))
    words = sorted({w for _, n in names for w in n.split(' ')})
    index = {w: i for i, w in enumerate(words)}
    assert len(words) < 65536 and all(len(w.encode()) < 256 for w in words)
    out = bytearray(struct.pack('<I', len(words)))
    for w in words:
        b = w.encode('ascii')
        out += bytes([len(b)]) + b
    out += struct.pack('<I', len(names))
    for cp, n in names:
        ws = n.split(' ')
        out += struct.pack('<IB', cp, len(ws)) + b''.join(struct.pack('<H', index[w]) for w in ws)
    with open(OUT, 'wb') as f:
        f.write(out)
    print(len(names), 'names,', len(words), 'words,', len(out), 'bytes, Unicode', unicodedata.unidata_version, '->', OUT)


if __name__ == '__main__':
    main()
