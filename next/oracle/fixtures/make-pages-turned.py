"""Fixtures for drawing on turned pages: pages.idml with other transforms on its spreads and pages.

pages-turned   the three spreads turned the way View > Rotate Spread View writes them (90 CW, 90 CCW, 180): the spread's
               ItemTransform is a pure quarter turn plus its place on the pasteboard (values copied from
               goldens/pages/rot-two, rot-last); nothing else changes.
pages-xform    transforms that View > Rotate Spread cannot make (an IDML written elsewhere): a spread turned 90 CW and scaled by
               1.5, a spread scaled by 2.
               (Measured and left out, see drawtools.turned.other in rules/drawtools.toml: a spread turned by 30 degrees, where
               InDesign's script draws a skewed quad, and a page with its own 90 turn, which InDesign prints turned.)

Usage: python -I next/oracle/fixtures/make-pages-turned.py   (no InDesign needed; ids stay those of pages.idml)
"""
import re
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def rewrite(src, dst, spreads, pages=None):
    pages = pages or {}
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, 'w') as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.startswith('Spreads/Spread_'):
                text = data.decode('utf-8')
                sid = re.search(r'<Spread Self="([^"]+)"', text).group(1)
                if sid in spreads:
                    text, n = re.subn(r'(<Spread Self="%s"[^>]*? ItemTransform=")[^"]*(")' % sid, lambda m: m.group(1) + spreads[sid] + m.group(2), text, count=1)
                    assert n == 1, sid
                for pid, matrix in pages.items():
                    text = re.sub(r'(<Page Self="%s"[^>]*? ItemTransform=")[^"]*(")' % pid, lambda m: m.group(1) + matrix + m.group(2), text, count=1)
                data = text.encode('utf-8')
            zi = zipfile.ZipInfo(info.filename, info.date_time)
            zi.compress_type = zipfile.ZIP_STORED if info.filename == 'mimetype' else zipfile.ZIP_DEFLATED
            zout.writestr(zi, data)


# 90 CW and 1.5 on the first spread, 2 on the third. The places are the ones InDesign wrote when it opened the file (it lays the
# spreads out again; a second open changes nothing).
XFORM_SPREADS = {'u147': '0 1.5 -1.5 0 0 0', 'u1a4': '2 0 0 2 0 2332'}


def main():
    src = HERE / 'pages.idml'
    rewrite(src, HERE / 'pages-turned.idml', {
        'u147': '0 1 -1 0 0 0',
        'u165': '0 -1 1 0 0 1236',
        'u1a4': '-1 0 0 -1 0 2144',
    })
    rewrite(src, HERE / 'pages-xform.idml', XFORM_SPREADS)


if __name__ == '__main__':
    main()
