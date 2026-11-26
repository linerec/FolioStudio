"""Original cut-paper illustrations for Riverbend Letters. All coordinates are in points."""
from math import cos, pi, sin

INK = "#193d43"
PINE = "#267366"
MINT = "#a9c9aa"
PAPER = "#f6eed7"
SUN = "#f4b549"
RED = "#ca5537"
BLUE = "#72aeb4"


def rect(x, y, w, h, color):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{color}"/>'


def circle(x, y, r, color):
    return f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r}" fill="{color}"/>'


def path(d, color, stroke="none", width=1):
    return f'<path d="{d}" fill="{color}" stroke="{stroke}" stroke-width="{width}"/>'


def line(x, y, x2, y2, color, width=2):
    return path(f"M{x:.2f} {y:.2f} L{x2:.2f} {y2:.2f}", "none", color, width)


def market():
    s = rect(0, 0, 600, 440, PAPER) + circle(458, 98, 61, SUN)
    s += path("M0 145 Q78 61 161 117 T313 113 T466 117 T610 98 V230 H0Z", MINT)
    s += rect(0, 170, 600, 270, BLUE)
    for i in range(48):
        x, y = (i * 83) % 610 - 10, 180 + (i * 37) % 143
        s += line(x, y, x + 11 + i % 11, y - 1, PAPER, 1.3)
    for i in range(14):
        x, h = i * 46 - 9, 19 + (i * 13) % 35
        s += rect(x, 166 - h, 35, h, [PINE, INK, RED][i % 3])
        s += path(f"M{x-3} {166-h} l20 -13 l21 13Z", INK)
        for j in (5, 16, 26):
            s += rect(x + j, 173 - h, 3, 6, SUN)
    s += path("M-20 218 Q203 149 372 189 L624 210 L624 217 L374 199 Q200 159 -20 230Z", INK)
    for x, y in [(32, 202), (124, 181), (241, 179), (365, 190), (492, 199)]:
        s += rect(x, y, 7, 32, INK)
    s += path("M-8 337 Q220 267 610 266 V450 H-8Z", PINE)
    for x, y, w, color in [(347, 233, 86, RED), (479, 219, 89, SUN), (188, 277, 120, SUN), (18, 307, 145, RED), (407, 305, 155, RED)]:
        h = w * .39
        s += rect(x+7, y+10, 4, h+19, INK) + rect(x+w-10, y+10, 4, h+19, INK)
        s += rect(x+4, y+h, w-8, 19, INK)
        for j in range(8):
            step = w/8
            s += path(f"M{x+12+j*(w-24)/8} {y-13} h{(w-24)/8+.3} l{(j+1)*3-12} 28 H{x+j*step}Z", color if j%2 == 0 else PAPER)
            s += circle(x+10+j*(w-20)/8, y+h-4, 4, SUN if j%3 else RED)
        s += rect(x+3, y+h+13, w-6, 6, color)
    for i in range(13):
        x, y = (i*97+31)%590, 341+(i*19)%78
        s += path(f"M{x-5} {y+18} q-1 -17 6 -18 q7 -1 8 17Z", [MINT, INK, SUN][i%3])
        s += circle(x+2, y-3, 3.7, PAPER) + line(x, y+17, x-2, y+28, INK, 2) + line(x+6,y+17,x+8,y+28,INK,2)
    return s


def river():
    s = rect(0, 0, 600, 440, MINT)
    s += path("M-20 30 Q160 10 162 125 T345 253 T640 321 L640 440 Q380 432 253 330 T66 183 T-20 144Z", BLUE)
    for i in range(80):
        x, y = (i*67)%600, (i*47)%440
        if (y < x*.5-40 or y > x*.55+189):
            s += path(f"M{x} {y} l8 -19 l9 19Z", PINE if i%4 else INK)
    for i in range(16):
        x, y = i*39-5, 38+(i*19)%55
        s += rect(x, y, 17, 14, PAPER) + path(f"M{x-3} {y} l12 -10 l12 10Z", RED)
    s += path("M-20 303 Q75 229 140 211 L359 127 L605 171", "none", PAPER, 12)
    s += line(130, 214, 287, 154, INK, 16) + line(130,214,287,154,SUN,6)
    for x,y in [(74,247),(147,208),(306,147),(429,140),(552,159)]:
        s += circle(x,y,11,RED) + circle(x,y,4,PAPER)
    return s


def still_life():
    s = rect(0,0,600,440, SUN) + path("M0 315 Q242 240 600 281 V450 H0Z", RED)
    for i in range(18):
        s += line(80 + i*20, 314, 15 + i*29, 440, PAPER, 1)
    s += path("M89 161 Q174 137 236 165 L221 314 Q174 339 108 308Z", PINE)
    s += path("M99 159 Q172 124 232 159 Q161 187 99 159Z", INK)
    for i in range(9):
        s += line(112+i*13,190,123+i*10,292,MINT,3)
    for x,y,r in [(284,276,47),(358,259,52),(432,280,43),(337,306,49),(405,321,45)]:
        s += circle(x+3,y+5,r,INK) + circle(x,y,r,SUN if x%2 else RED)
        s += path(f"M{x} {y-r+3} q12 -33 38 -22 q-14 21 -38 22Z", PINE)
        for i in range(6):
            s += circle(x-r*.5+i*3,y+i*3,1.3,PAPER)
    s += path("M233 309 Q356 338 505 306 Q485 407 370 401 Q271 398 233 309Z", PAPER)
    s += path("M249 331 Q358 365 486 330", "none", BLUE, 6)
    return s


def wheel():
    s = rect(0,0,600,440,INK)
    for cx,cy,r in [(164,238,86),(427,238,86)]:
        s += circle(cx,cy,r+7,MINT) + circle(cx,cy,r,INK)
        for i in range(24):
            a = i*pi/12
            s += line(cx,cy,cx+r*cos(a),cy+r*sin(a),MINT,1.3)
        s += circle(cx,cy,8,SUN)
    s += path("M164 238 L228 126 L294 240 Z M228 126 L394 108 L294 240 M427 238 L383 80 L414 69 M228 126 L209 94", "none", RED, 11)
    s += line(183,94,231,94,PAPER,8) + circle(294,240,15,SUN) + line(294,240,311,259,PAPER,4)
    for i in range(5):
        s += line(75+i*47,367,266+i*47,367,SUN,1.4)
    return s


def rain():
    s = rect(0,0,600,440,BLUE)
    for i in range(135):
        x,y=(i*43)%610,(i*97)%440
        s += line(x,y,x-4,y+9,PAPER,1)
    for x,y,r,c in [(120,160,93,PINE),(327,209,113,RED),(500,94,73,SUN)]:
        s += line(x,y,x,y+85,INK,5) + path(f"M{x} {y+84} q-2 18 -14 12", "none", INK, 5)
        s += path(f"M{x-r} {y} q{r*.36} {-r*1.24} {r} {-r*.81} q{r*.73} {-r*.22} {r} {r*.81} q{-r*.24} -14 {-r*.48} 0 q{-r*.26} -15 {-r*.52} 0 q{-r*.24} -17 {-r*.51} 0 q{-r*.24} -14 {-r*.49} 0Z",c)
        s += path(f"M{x} {y-r*.81} q{-r*.31} 6 {-r*.5} {r*.8} M{x} {y-r*.81} q{r*.35} 9 {r*.52} {r*.81}", "none",PAPER,1.5)
    return s


ILLUSTRATIONS = {
    "market": (market, "능선, 강 위의 다리, 층층이 놓인 천막과 장바구니를 든 사람들을 다각형과 선으로 그렸다."),
    "river": (river, "가상의 강줄기와 다리, 지붕과 산책길을 곡선과 기호로 구성했다."),
    "pantry": (still_life, "옹기와 과일을 놓은 식탁을 원, 곡선, 빗금으로 그렸다."),
    "cycle": (wheel, "두 바퀴의 살을 각도로 나누고 프레임과 페달을 선으로 연결했다."),
    "rain": (rain, "세 우산의 골과 빗방울을 반복하는 선과 면으로 그렸다."),
}


def write_assets(folder):
    folder.mkdir(parents=True, exist_ok=True)
    for name, (draw, _) in ILLUSTRATIONS.items():
        (folder / f"{name}.svg").write_text(
            '<svg xmlns="http://www.w3.org/2000/svg" width="600" height="440" viewBox="0 0 600 440">' + draw() + '</svg>\n', encoding="utf-8", newline="\n")
