"""Check the normal and delay-load DLL imports in a Windows package."""
from pathlib import Path
import struct


# Inbox modules on standard Windows 10/11. Windows N needs the Media Feature
# Pack for the Media Foundation modules that Electron imports.
SYSTEM_DLLS = frozenset('''
    advapi32.dll bcrypt.dll bcryptprimitives.dll bthprops.cpl cfgmgr32.dll
    comctl32.dll comdlg32.dll crypt32.dll cryptui.dll d3d11.dll d3d12.dll
    dbghelp.dll dcomp.dll dhcpcsvc.dll dwrite.dll dwmapi.dll dxgi.dll
    gdi32.dll hid.dll iphlpapi.dll kernel32.dll mf.dll mfplat.dll
    mfreadwrite.dll mmdevapi.dll ncrypt.dll ntdll.dll ole32.dll oleacc.dll
    oleaut32.dll pdh.dll propsys.dll rpcrt4.dll secur32.dll setupapi.dll
    shell32.dll shlwapi.dll tbs.dll uiautomationcore.dll urlmon.dll user32.dll
    userenv.dll uxtheme.dll version.dll wevtapi.dll winhttp.dll winmm.dll
    winspool.drv wintrust.dll winusb.dll ws2_32.dll wtsapi32.dll
'''.split())
SYSTEM_PREFIXES = ('api-ms-win-', 'ext-ms-')


def read_imports(path):
    """Return the DLL names in both import tables, preserving their spelling."""
    data = Path(path).read_bytes()
    if data[:2] != b'MZ':
        raise ValueError('missing DOS signature')
    pe, = struct.unpack_from('<I', data, 0x3c)
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('missing PE signature')
    section_count, = struct.unpack_from('<H', data, pe + 6)
    optional_size, = struct.unpack_from('<H', data, pe + 20)
    optional = pe + 24
    magic, = struct.unpack_from('<H', data, optional)
    if magic == 0x10b:  # PE32
        directories_at = 96
        image_base, = struct.unpack_from('<I', data, optional + 28)
    elif magic == 0x20b:  # PE32+
        directories_at = 112
        image_base, = struct.unpack_from('<Q', data, optional + 24)
    else:
        raise ValueError(f'unknown PE optional header magic {magic:#x}')
    directory_count, = struct.unpack_from('<I', data, optional + directories_at - 4)
    if directories_at + directory_count * 8 > optional_size:
        raise ValueError('data directories extend beyond the optional header')
    section_table = optional + optional_size
    if section_table + section_count * 40 > len(data):
        raise ValueError('truncated section table')
    headers_size, = struct.unpack_from('<I', data, optional + 60)
    sections = [(0, 0, headers_size)]
    for index in range(section_count):
        rva, size, offset = struct.unpack_from('<III', data, section_table + index * 40 + 12)
        sections.append((rva, offset, size))

    def file_span(rva):
        for base, offset, size in sections:
            if base <= rva < base + size:
                start, end = offset + rva - base, min(offset + size, len(data))
                if start < end:
                    return start, end
        raise ValueError(f'RVA {rva:#x} has no file data')

    def import_names(index, fields, name_field):
        if index >= directory_count:
            return
        rva, size = struct.unpack_from('<II', data, optional + directories_at + index * 8)
        if not (rva or size):
            return
        if not rva:
            raise ValueError(f'data directory {index} is missing its RVA')
        descriptor = struct.Struct(f'<{fields}I')
        start, end = file_span(rva)
        end = min(end, start + size)
        for offset in range(start, end - descriptor.size + 1, descriptor.size):
            entries = descriptor.unpack_from(data, offset)
            if not any(entries):
                return
            name_rva = entries[name_field]
            if index == 13:  # Legacy delay descriptors encode the name as a VA.
                if entries[0] == 0:
                    name_rva -= image_base
                elif entries[0] != 1:
                    raise ValueError('unknown delay import attributes')
            if name_rva <= 0:
                raise ValueError('missing import name RVA')
            name_at, name_end = file_span(name_rva)
            terminator = data.find(b'\0', name_at, name_end)
            if terminator <= name_at:
                raise ValueError('empty or unterminated import name')
            yield data[name_at:terminator].decode('ascii')
        raise ValueError(f'data directory {index} has no terminating import descriptor')

    return [*import_names(1, 5, 3), *import_names(13, 8, 1)]


def dependency_errors(folder):
    """Return unreadable PE files and unresolved imports, with package-relative paths."""
    folder = Path(folder)
    errors = []
    for path in sorted(folder.rglob('*')):
        if path.suffix.lower() not in ('.exe', '.dll') or not path.is_file():
            continue
        relative = path.relative_to(folder)
        try:
            imports = read_imports(path)
        except (OSError, ValueError, struct.error) as error:
            errors.append(f'{relative}: cannot read PE imports: {error}')
            continue
        siblings = {p.name.lower() for p in path.parent.iterdir() if p.is_file()}
        for name in sorted({name.lower(): name for name in imports}.values(), key=str.lower):
            key = name.lower()
            if key not in siblings and key not in SYSTEM_DLLS and not key.startswith(SYSTEM_PREFIXES):
                errors.append(f'{relative}: missing DLL {name}')
    return errors
