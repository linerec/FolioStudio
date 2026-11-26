"""Makes magazine.sqlite, the small SQLite test database of the data merge database source (dbmerge-a).
Korean text, NULL, dates, numbers, picture paths, a view. Run: python -I make_fixture.py (writes next to this file).
The same rows seed the PostgreSQL and MariaDB integration runs (rows() is imported by tools/dbmerge-servers.py)."""
import os
import sqlite3
import sys

ROWS = [
    # id, title, price, issue_date, status, page, photo
    (1, '가을 특집, "단풍"', 12000, '2026-11-03', '게재', 12, 'images/autumn.jpg'),
    (2, '겨울 코트', 89000.5, '2026-11-17', '확정', 3, 'images/coat.jpg'),
    (3, '새해 달력', None, '2026-12-01', '게재', 7, None),
    (4, None, 5000, '2026-10-31', '보류', 1, 'images/none.jpg'),
    (5, '첫 줄\n둘째 줄', 0, '2026-11-30', '게재', 20, ''),
    (6, '50% 할인_특가!', 1500, '2026-11-01', '확정', 5, 'images/sale.png'),
]


def rows():
    return ROWS


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, 'magazine.sqlite')
    if os.path.exists(path):
        os.remove(path)
    con = sqlite3.connect(path)
    con.execute('CREATE TABLE ads (id INTEGER PRIMARY KEY, title TEXT, price NUMERIC, issue_date TEXT, status TEXT, page INTEGER, "@photo" TEXT)')
    con.executemany('INSERT INTO ads VALUES (?, ?, ?, ?, ?, ?, ?)', ROWS)
    con.execute("CREATE VIEW published AS SELECT id, title, page FROM ads WHERE status = '게재'")
    con.execute('CREATE TABLE "광고 목록" ("번호" INTEGER, "제목" TEXT)')
    con.execute('INSERT INTO "광고 목록" VALUES (1, \'한글 표 이름\')')
    con.commit()
    con.execute('VACUUM')
    con.close()
    print(path)


if __name__ == '__main__':
    sys.exit(main())
