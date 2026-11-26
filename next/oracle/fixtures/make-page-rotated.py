"""Fixture for drawing on pages turned inside the IDML (pen-c2): basic.idml with the first page turned 90 degrees clockwise about its middle
and the last page (u116, the right page of the second spread) turned 30 degrees about its middle, by their own Page ItemTransform (an IDML
written elsewhere: View > Rotate Spread turns the spread, not the page). Folio shows such a page as its bounding box on the spread with the
objects as they lie there; the Pen, the Pencil and the point tools read the pointer in that box.

Usage: python -I next/oracle/fixtures/make-page-rotated.py   (no InDesign needed; ids stay those of basic.idml)
"""
import importlib.util
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('make_pages_turned', HERE / 'make-pages-turned.py')
mpt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mpt)


def turn_about(deg, inner_mid, spread_mid):
    """The matrix that turns a page by `deg` (clockwise on the screen) so its middle `inner_mid` (page inner coordinates) lands on `spread_mid`."""
    a, b = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    c, d = -b, a
    tx = spread_mid[0] - (a * inner_mid[0] + c * inner_mid[1])
    ty = spread_mid[1] - (b * inner_mid[0] + d * inner_mid[1])
    return ' '.join(f'{v:.6f}'.rstrip('0').rstrip('.') if '.' in f'{v:.6f}' else str(v) for v in (a, b, c, d, tx, ty))


def main():
    pages = {
        # first page: bounds -24.945 0 816.945 595.276, placed at y -396: its middle is (297.638, 396) inside, (297.638, 0) on the spread
        'ufc': turn_about(90, (297.638, 396.0), (297.638, 0.0)),
        # the right page of the second spread: bounds 0 0 841.89 595.276 at y -420.945
        'u116': turn_about(30, (297.638, 420.945), (297.638, 0.0)),
    }
    mpt.rewrite(HERE / 'basic.idml', HERE / 'page-rotated.idml', {}, pages)
    print(pages)


if __name__ == '__main__':
    main()
