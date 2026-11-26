"""Our deterministic PDF/AI placement fixtures (no third-party fonts or artwork)."""
from pathlib import Path


def pdf(pages):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'']
    kids = []
    for boxes, content in pages:
        page_id = len(objects) + 1
        kids.append(f'{page_id} 0 R')
        attrs = ' '.join('/%s [%s]' % (k, ' '.join(map(str, v))) for k, v in boxes.items())
        objects.append(('<< /Type /Page /Parent 2 0 R %s /Resources << /ExtGState << /half << /Type /ExtGState /ca 0.5 >> >> >> /Contents %d 0 R >>' % (attrs, page_id + 1)).encode())
        data = content.encode('ascii')
        objects.append(b'<< /Length %d >>\nstream\n' % len(data) + data + b'\nendstream')
    objects[1] = ('<< /Type /Pages /Kids [%s] /Count %d >>' % (' '.join(kids), len(kids))).encode()
    out = bytearray(b'%PDF-1.4\n%\xe2\xe3\xcf\xd3\n')
    offsets = []
    for i, obj in enumerate(objects, 1):
        offsets.append(len(out))
        out += b'%d 0 obj\n' % i + obj + b'\nendobj\n'
    xref = len(out)
    out += b'xref\n0 %d\n0000000000 65535 f \n' % (len(objects) + 1)
    out += b''.join(b'%010d 00000 n \n' % o for o in offsets)
    out += b'trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n' % (len(objects) + 1, xref)
    return out


def main():
    out = Path(__file__).resolve().parent / 'Links'
    boxes = dict(MediaBox=[0, 0, 240, 300], CropBox=[10, 30, 230, 290],
                 ArtBox=[30, 100, 200, 270], TrimBox=[20, 60, 220, 280], BleedBox=[15, 45, 225, 285])
    content = '0 1 1 0 k 40 180 70 90 re f\n1 1 0 0 k 140 110 50 90 re f\n'
    first = (boxes, content)
    single = pdf([first])
    (out / 'pdfplace-single.pdf').write_bytes(single)
    (out / 'pdfplace-compatible.ai').write_bytes(single)
    (out / 'pdfplace-multi.pdf').write_bytes(pdf([first,
        (dict(MediaBox=[100, 50, 460, 250], CropBox=[110, 60, 450, 240], ArtBox=[130, 80, 410, 220],
              TrimBox=[120, 70, 440, 230], BleedBox=[115, 65, 445, 235]), '0 0 1 0 k 140 100 80 100 re f\n1 0 1 0 k 280 90 120 100 re f\n'),
        (dict(MediaBox=[0, 0, 180, 240]), content)]))
    (out / 'pdfplace-alpha.pdf').write_bytes(pdf([(boxes, 'q /half gs\n' + content + 'Q\n')]))
    # Only a Windows system font is embedded, as a small test subset.
    from reportlab.pdfgen.canvas import Canvas
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    pdfmetrics.registerFont(TTFont('TestArial', 'C:/Windows/Fonts/arial.ttf'))
    c = Canvas(str(out / 'pdfplace-font.pdf'), pagesize=(240, 300), invariant=1, initialFontName='TestArial')
    c.setFont('TestArial', 22)
    c.setFillColorCMYK(0, 0, 0, 1)
    c.drawString(20, 220, 'PDF ad 2026')
    c.setFillColorCMYK(1, 0, 0, 0)
    c.rect(20, 100, 180, 70, fill=1, stroke=0)
    c.save()
    print('PDF placement fixtures generated:', out)


if __name__ == '__main__':
    main()
