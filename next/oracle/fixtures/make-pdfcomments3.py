"""Builds fixtures/pdfcomments3.idml from text-format.idml: the same document with imported PDF comments in its designmap
(PDFComment, Reply and the PDFComment*Destination elements, spelled as InDesign wrote them: rules/pdfcomments-persist.designmap.xml).
Stories of text-format: uf4 "Alpha bold ...", u10a "Destination text.", page uf2, frame u106. usage: python -I make-pdfcomments3.py"""
import os, re, zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
FILE = '$ID//oracle/Links/notes.pdf'


def comment(n, dest, kind, content, status='OpenStatus', orphan=False, applied=None, reviewer='Reviewer 1', date='2026-11-07T08:45:00', replies=()):
    attrs = (f'Self="u{n:x}" CommentReviewer="{reviewer}" CommentContent="{content}" CommentDate="{date}" CommentType="{kind}" '
             f'CommentFilePath="{FILE}" CommentStatus="{status}" CommentIsOrphan="{str(orphan).lower()}"')
    if applied is not None:
        attrs += f' CommentIsApplied="{str(applied).lower()}"'
    attrs += f' CommentDestination="{dest}"'
    if not replies:
        return f'\t<PDFComment {attrs} />\n'
    inner = ''.join(f'\t\t<Reply Self="u{n + 1 + i:x}" ReplyReviewer="{who}" ReplyContent="{text}" ReplyDate="{when}" />\n' for i, (who, text, when) in enumerate(replies))
    return f'\t<PDFComment {attrs}>\n{inner}\t</PDFComment>\n'


def page_dest(n, page, rect):
    l, t, r, b = rect
    return (f'\t<PDFCommentPageDestination Self="u{n:x}" CommentPageDestinationPage="{page}" CommentPageDestinationTargetPoint="0 0">\n\t\t<Properties>\n'
            f'\t\t\t<CommentPageDestinationHighlightRect Left="{l}" Top="{t}" Right="{r}" Bottom="{b}" />\n\t\t</Properties>\n\t</PDFCommentPageDestination>\n')


def item_dest(n, item, rect):
    l, t, r, b = rect
    return (f'\t<PDFCommentPageItemDestination Self="u{n:x}" CommentPageItemDestinationPageItem="{item}" CommentPageItemDestinationTargetPoint="0 0">\n\t\t<Properties>\n'
            f'\t\t\t<CommentPageItemDestinationHighlightRect Left="{l}" Top="{t}" Right="{r}" Bottom="{b}" />\n\t\t</Properties>\n\t</PDFCommentPageItemDestination>\n')


def text_dest(n, story, start, end):
    return f'\t<PDFCommentTextDestination Self="u{n:x}" CommentTextDestinationStory="{story}" CommentTextDestinationStartIndex="{start}" CommentTextDestinationEndIndex="{end}" />\n'


def build():
    src = os.path.join(HERE, 'text-format.idml')
    out = os.path.join(HERE, 'pdfcomments3.idml')
    base = 0x300
    # destinations by kind, then the comments (InDesign's order)
    dests = (page_dest(base + 0, 'uf2', (300, 20, 316, 36))
             + item_dest(base + 2, 'u106', (-270, -136, -6, -96))
             + text_dest(base + 3, 'u10a', 0, 11) + text_dest(base + 4, 'u10a', 12, 16) + text_dest(base + 5, 'uf4', 0, 5))
    comments = ''.join([
        comment(base + 10, f'u{base + 0:x}', 'StickyNoteType', 'Note comment', replies=[('Reviewer 2', 'Reply to note', '2026-11-07T20:45:00')]),
        comment(base + 20, f'u{base + 3:x}', 'HighlightTextType', 'Highlight comment', date='2026-11-07T09:45:00'),
        comment(base + 22, f'u{base + 4:x}', 'StrikethroughTextType', 'Strike comment', applied=False, date='2026-11-07T11:45:00', reviewer='Reviewer 2'),
        comment(base + 24, f'u{base + 2:x}', 'TextBoxType', 'Text box comment', date='2026-11-07T13:45:00'),
        comment(base + 28, f'u{base + 5:x}', 'UnderlineTextType', 'Underline comment', status='ResolvedStatus', date='2026-11-07T10:45:00'),
    ])
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(out, 'w') as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename == 'designmap.xml':
                text = data.decode('utf-8')
                at = text.find('\t<IndexingSortOption')
                assert at > 0, 'no IndexingSortOption in the designmap'
                text = text[:at] + dests.replace('\n', '\r\n' if '\r\n' in text else '\n') + comments.replace('\n', '\r\n' if '\r\n' in text else '\n') + text[at:]
                data = text.encode('utf-8')
            zout.writestr(info.filename, data, compress_type=zipfile.ZIP_STORED if info.filename == 'mimetype' else zipfile.ZIP_DEFLATED)
    print('wrote', out)


def normalize():
    """One open/export round in InDesign makes the object ids a fixed point (as `capture.py build` does for the built fixtures).
    Run it inside the InDesign lock: python -X utf8 <ig-lock.py> python -I next/oracle/fixtures/make-pdfcomments3.py --normalize"""
    import json, shutil, sys
    sys.path.insert(0, os.path.dirname(HERE))
    import capture
    out = os.path.join(HERE, 'pdfcomments3.idml')
    tmp = [os.path.join(capture.WORK, 'build', f'pdfcomments3-{k}.idml') for k in (1, 2)]
    fwd = capture.fwd
    script = os.path.join(os.path.dirname(HERE), 'indesign', 'reexport.jsx')
    first = capture.run_jsx(script, {'src': fwd(out), 'out': [fwd(tmp[0])]}, os.path.join(capture.WORK, 'build', 'pdfcomments3-reexport.json'))
    assert first.get('runs') == ['ok'], first
    shutil.copy(tmp[0], out)
    second = capture.run_jsx(script, {'src': fwd(out), 'out': [fwd(tmp[1])]}, os.path.join(capture.WORK, 'build', 'pdfcomments3-fixed.json'))
    assert second.get('runs') == ['ok'], second
    selfs = lambda f: sorted(set(re.findall(r'Self="([^"]+)"', ''.join(capture.parts(f).values()))))
    assert selfs(out) == selfs(tmp[1]), 'object ids are not stable after a re-export'
    print('normalized', out, len(selfs(out)), 'ids; PDFComments:', ''.join(capture.parts(out).values()).count('<PDFComment '))


if __name__ == '__main__':
    import sys
    normalize() if '--normalize' in sys.argv else build()
