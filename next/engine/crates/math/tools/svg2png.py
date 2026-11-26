"""Tiny rasteriser for the SVG folio-math writes (path M L Q C Z, rect; y turned by scale(1,-1)).
Usage: python -B svg2png.py out.png scale in1.svg [in2.svg ...]   (stacks the drawings, for looking at them)"""
import re, sys
from PIL import Image, ImageDraw, ImageChops

def flatten(d, k):
    subs, cur, pos = [], [], (0, 0)
    for cmd, args in re.findall(r'([MLQCZ])([^MLQCZ]*)', d):
        n = [float(v) for v in args.replace(',', ' ').split()]
        if cmd == 'M':
            if cur: subs.append(cur)
            pos = (n[0], n[1]); cur = [pos]
        elif cmd == 'L':
            pos = (n[0], n[1]); cur.append(pos)
        elif cmd == 'Q':
            p0, p1, p2 = pos, (n[0], n[1]), (n[2], n[3])
            for i in range(1, 9):
                t = i / 8
                cur.append(((1-t)**2*p0[0]+2*(1-t)*t*p1[0]+t*t*p2[0], (1-t)**2*p0[1]+2*(1-t)*t*p1[1]+t*t*p2[1]))
            pos = p2
        elif cmd == 'C':
            p0, p1, p2, p3 = pos, (n[0], n[1]), (n[2], n[3]), (n[4], n[5])
            for i in range(1, 9):
                t = i / 8
                a, b, c, e = (1-t)**3, 3*(1-t)**2*t, 3*(1-t)*t*t, t**3
                cur.append((a*p0[0]+b*p1[0]+c*p2[0]+e*p3[0], a*p0[1]+b*p1[1]+c*p2[1]+e*p3[1]))
            pos = p3
        elif cmd == 'Z':
            if cur: subs.append(cur); cur = []
    if cur: subs.append(cur)
    return subs

def draw(svg, scale):
    vb = [float(v) for v in re.search(r'viewBox="([^"]+)"', svg).group(1).split()]
    x0, y0, w, h = vb
    W, H = max(1, int(w * scale)), max(1, int(h * scale))
    img = Image.new('L', (W, H), 255)
    def tx(p): return ((p[0] - x0) * scale, (-p[1] - y0) * scale)
    for d in re.findall(r'<path d="([^"]*)"', svg):
        mask = Image.new('L', (W, H), 0)
        for sub in flatten(d, scale):
            m = Image.new('L', (W, H), 0)
            ImageDraw.Draw(m).polygon([tx(p) for p in sub], fill=255)
            mask = ImageChops.logical_xor(mask.convert('1'), m.convert('1')).convert('L')
        img.paste(0, mask=mask)
    for a in re.findall(r'<rect ([^>]*)>', svg):
        g = dict(re.findall(r'(\w+)="([^"]*)"', a))
        x, y, rw, rh = (float(g[k]) for k in ('x', 'y', 'width', 'height'))
        p, q = tx((x, y)), tx((x + rw, y + rh))
        ImageDraw.Draw(img).rectangle([min(p[0], q[0]), min(p[1], q[1]), max(p[0], q[0]), max(p[1], q[1])], fill=0)
    return img

if __name__ == '__main__':
    out, scale, files = sys.argv[1], float(sys.argv[2]), sys.argv[3:]
    imgs = [draw(open(f, encoding='utf8').read(), scale) for f in files]
    W = max(i.width for i in imgs) + 20
    H = sum(i.height + 14 for i in imgs) + 6
    sheet = Image.new('L', (W, H), 255)
    y = 6
    for i, im in enumerate(imgs):
        sheet.paste(im, (10, y)); ImageDraw.Draw(sheet).text((W - 80, y), files[i].split('/')[-1][:12], fill=120); y += im.height + 14
    sheet.save(out)
