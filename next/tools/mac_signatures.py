"""Offline integrity checks for thin 64-bit Mach-O ad-hoc signatures and bundle seals.

This verifies digests, not Apple's trust policy/Gatekeeper. Native codesign --verify
--deep --strict remains a required Mac smoke check.
CodeDirectory format: Apple's Security/OSX/libsecurity_codesigning/lib/cs_blobs.h.
"""
import hashlib
from pathlib import Path
import plistlib
import struct


def check(condition, message):
    if not condition:
        raise ValueError(message)


def signature(path):
    data = Path(path).read_bytes()
    check(data[:4] == b'\xcf\xfa\xed\xfe', f'Not thin 64-bit Mach-O: {path}')
    ncmds = struct.unpack_from('<I', data, 16)[0]
    at = 32
    offset = length = 0
    for _ in range(ncmds):
        cmd, size = struct.unpack_from('<II', data, at)
        check(size >= 8, f'Invalid load command: {path}')
        if cmd == 0x1d:  # LC_CODE_SIGNATURE
            offset, length = struct.unpack_from('<II', data, at + 8)
        at += size
    check(offset > 0 and offset + length <= len(data), f'Missing/truncated signature: {path}')
    sig = data[offset:offset + length]
    magic, size, count = struct.unpack_from('>III', sig)
    check(magic == 0xfade0cc0 and size <= len(sig), f'Invalid signature superblob: {path}')
    blobs = {}
    for i in range(count):
        slot, pos = struct.unpack_from('>II', sig, 12 + i * 8)
        _, blobsize = struct.unpack_from('>II', sig, pos)
        check(pos + blobsize <= size, f'Invalid signature blob: {path}')
        blobs[slot] = sig[pos:pos + blobsize]
    return data, offset, blobs


def verify_code(path, special=None):
    data, signature_offset, blobs = signature(path)
    directories = [b for slot, b in blobs.items() if slot == 0 or 0x1000 <= slot < 0x1005]
    check(bool(directories), f'Missing CodeDirectory: {path}')
    cdhashes = []
    for cd in directories:
        magic, size, version, flags, hashes, ident, nspecial, ncode, limit = struct.unpack_from('>9I', cd)
        hashsize, kind, _, pageshift = struct.unpack_from('4B', cd, 36)
        check(magic == 0xfade0c02 and flags & 2, f'Not ad-hoc signed: {path}')
        check(limit == signature_offset, f'Unsigned executable data: {path}')
        algorithm = {1: 'sha1', 2: 'sha256', 3: 'sha256', 4: 'sha384'}.get(kind)
        check(algorithm is not None, f'Unsupported hash: {kind}')
        hash_bytes = lambda b: hashlib.new(algorithm, b).digest()[:hashsize]
        pagesize = (1 << pageshift) if pageshift else limit
        check(ncode == (limit + pagesize - 1) // pagesize, f'Wrong page count: {path}')
        for i in range(ncode):
            expected = cd[hashes + i * hashsize:hashes + (i + 1) * hashsize]
            actual = hash_bytes(data[i * pagesize:min((i + 1) * pagesize, limit)])
            check(expected == actual, f'Code page {i} digest mismatch: {path}')
        values = {slot: blobs[slot] for slot in (2, 5, 7) if slot in blobs}
        values.update(special or {})
        for slot, content in values.items():
            check(slot <= nspecial, f'Missing special slot {slot}: {path}')
            expected = cd[hashes - slot * hashsize:hashes - (slot - 1) * hashsize]
            check(expected == hash_bytes(content), f'Special slot {slot} digest mismatch: {path}')
        cdhashes.append(hashlib.new(algorithm, cd).digest()[:20])
    return cdhashes


def bundle_main(bundle):
    bundle = Path(bundle)
    for rel, executable_root in [('Contents/Info.plist', 'Contents/MacOS'),
                                 ('Resources/Info.plist', ''), ('Info.plist', '')]:
        info = bundle / rel
        if info.is_file():
            metadata = plistlib.loads(info.read_bytes())
            if metadata.get('CFBundleExecutable'):
                return bundle / executable_root / metadata['CFBundleExecutable'], info
    raise ValueError(f'Missing main executable/Info.plist: {bundle}')


def verify_resources(app):
    """Check regular files, link payloads, nested cdhashes and main special slots."""
    import os
    checked = 0
    for folder, dirs, files in os.walk(app, followlinks=False):
        dirs[:] = [d for d in dirs if not (Path(folder) / d).is_symlink()]
        if Path(folder).name != '_CodeSignature' or 'CodeResources' not in files:
            continue
        seal = Path(folder) / 'CodeResources'
        base = seal.parent.parent
        resource_data = seal.read_bytes()
        entries = plistlib.loads(resource_data)['files2']
        for name, value in entries.items():
            check('\\' not in name, f'Windows path in resource seal: {name}')
            path = base.joinpath(*name.split('/'))
            if 'symlink' in value:
                check(path.is_symlink(), f'Not a link: {path}')
                check(os.readlink(path).replace('\\', '/') == value['symlink'], f'Link seal mismatch: {path}')
            elif 'cdhash' in value:
                exe = bundle_main(path)[0] if path.is_dir() else path
                check(value['cdhash'] in verify_code(exe), f'Nested signature mismatch: {path}')
            else:
                content = path.read_bytes()
                for key, algorithm in [('hash', 'sha1'), ('hash2', 'sha256')]:
                    if key in value:
                        check(hashlib.new(algorithm, content).digest() == value[key], f'Resource digest mismatch: {path}')
            checked += 1
        bundle = base.parent if base.name == 'Contents' else base
        exe, info = bundle_main(bundle)
        verify_code(exe, {1: info.read_bytes(), 3: resource_data})
    check(checked > 0, 'No bundle resource seals found')
    return checked
