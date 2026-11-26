"""Extractable text: does a sample of story paragraphs from the IDML come out of the PDF?

Usage: python textcheck.py <document.idml> <out.json> <pdf> [<pdf>...]

Paragraphs (text between <Br/>, 20+ characters, without tabs/anchors/page numbers) are sampled
deterministically from the stories. Each is searched in the PDF's PyMuPDF text with all
whitespace and hyphens removed on both sides (line breaks and hyphenation differ), and also
with spaces kept (words joined by single spaces) to check that word spaces extract.
A paragraph counts as present when found on the page text of any page.
"""
import sys, json, re, zipfile, random
sys.stdout.reconfigure(encoding='utf-8')
import fitz


def paragraphs(idml):
    z = zipfile.ZipFile(idml)
    out = []
    for n in sorted(z.namelist()):
        if not n.startswith('Stories/'):
            continue
        x = z.read(n).decode('utf-8', 'replace')
        x = re.sub(r'<\?ACE [0-9]+\?>', '￼', x)
        parts = re.split(r'<Br\s*/>', x)
        for p in parts:
            t = ''.join(re.findall(r'<Content>(.*?)</Content>', p, re.S))
            t = t.replace('&amp;', '&').replace('&lt;', '<').replace('&gt;', '>').replace('&quot;', '"').replace('&apos;', "'")
            t = t.replace(' ', ' ')
            if len(t) >= 20 and '\t' not in t and '￼' not in t:
                out.append((n, t))
    return out


def squash(s):
    return re.sub(r'[\s­\-‐‑]+', '', s)


def spaced(s):
    return re.sub(r'\s+', ' ', s).strip()


def main():
    idml, out = sys.argv[1], sys.argv[2]
    pdfs = sys.argv[3:]
    paras = paragraphs(idml)
    rnd = random.Random(347)
    sample = rnd.sample(paras, min(400, len(paras)))
    texts = {}
    for f in pdfs:
        d = fitz.open(f)
        pages = [p.get_text() for p in d]
        texts[f] = ('\n'.join(pages), ''.join(squash(t) for t in pages), [spaced(t) for t in pages])
    rows = []
    for story, t in sample:
        row = {'story': story, 'text': t[:80]}
        for f in pdfs:
            full, sq, sp = texts[f]
            row[f] = {'squashed': squash(t) in sq, 'spaced': any(spaced(t)[:60] in p for p in sp)}
        rows.append(row)
    summary = {'sampled': len(sample), 'paragraphs': len(paras)}
    # only paragraphs any PDF shows (overset / pasteboard text is in neither)
    shown = [r for r in rows if any(r[f]['squashed'] for f in pdfs)]
    summary['shownInAnyPdf'] = len(shown)
    for f in pdfs:
        summary[f] = {
            'presentSquashed': sum(r[f]['squashed'] for r in shown),
            'presentWithSpaces': sum(r[f]['spaced'] for r in shown),
            'missing': [r['text'] for r in shown if not r[f]['squashed']][:8],
        }
    json.dump({'summary': summary, 'rows': rows}, open(out, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
    print(json.dumps(summary, ensure_ascii=False, indent=1)[:3000])


main()
