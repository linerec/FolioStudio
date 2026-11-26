"""Build the November issue using a Folio engine and the bundled OFL fonts.

Usage: python riverbend.py --engine PATH --output NEW_PROJECT --work WORK_DIRECTORY
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import uuid
import zipfile

from riverbend_art import ILLUSTRATIONS, write_assets
from riverbend_layout import composition
from riverbend_document import FACES, xml_zip

PROJECT_NAME = "물굽이 서간"
SAMPLE = Path(__file__).resolve().parents[3] / "samples" / "riverbend-letters"


class Engine:
    def __init__(self, exe, env):
        self.p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env)
        self.serial = 0
        while self.read()[0].get("event") != "ready":
            pass

    def read(self):
        length = self.p.stdout.read(4)
        if len(length) != 4:
            raise RuntimeError("Engine closed its output")
        header = json.loads(self.p.stdout.read(struct.unpack("<I",length)[0]))
        return header,self.p.stdout.read(header.get("bodyBytes",0))

    def ask(self,cmd,**args):
        self.serial += 1
        message={"id":self.serial,"cmd":cmd,**args}
        self.p.stdin.write((json.dumps(message)+"\n").encode())
        self.p.stdin.flush()
        while True:
            header,body=self.read()
            if header.get("id")==self.serial and "event" not in header:
                if not header.get("ok"):
                    raise RuntimeError(f"{cmd}: {header}")
                return header,body

    def close(self):
        self.p.stdin.write(b'{"cmd":"quit"}\n')
        self.p.stdin.close()
        self.p.wait(timeout=30)


def json_bytes(value):
    return (json.dumps(value,ensure_ascii=False,indent=2)+"\n").encode()


def font_version(data):
    tables = struct.unpack_from(">H", data, 4)[0]
    for index in range(tables):
        tag, _, offset, _ = struct.unpack_from(">4sIII", data, 12 + 16 * index)
        if tag == b"name":
            _, count, strings = struct.unpack_from(">HHH", data, offset)
            for n in range(count):
                platform, _, _, name, length, start = struct.unpack_from(">6H", data, offset + 6 + 12*n)
                if platform == 3 and name == 5:
                    pos = offset + strings + start
                    return data[pos:pos+length].decode("utf-16-be")
    raise ValueError("Missing TrueType version")


def finish_project(staging, output):
    # A published issue has stable IDs, dates and references to its distributed font files.
    date = "2026-11-01T00:00:00Z"
    project = json.loads((staging/"folio.project").read_bytes())
    project.update(id=str(uuid.uuid5(uuid.NAMESPACE_URL, "urn:folio:riverbend-letters")), created=date)
    with zipfile.ZipFile(staging/"november.folio") as archive:
        parts = {k: archive.read(k) for k in archive.namelist()}
    manifest = json.loads(parts["folio/manifest.json"])
    report = json.loads(parts["folio/import-report.json"])
    old_document = manifest["id"]
    old_project = manifest["project"]
    manifest.update(project=project["id"], id=str(uuid.uuid5(uuid.UUID(project["id"]), "november")), created=date, saved=date)
    identifiers = {old_document: manifest["id"], old_project: project["id"]}
    for graphic in manifest["graphics"]:
        stable = uuid.uuid5(uuid.UUID(manifest["id"]), graphic["self"])
        original = graphic["folioId"]
        graphic["folioId"] = "g" + stable.hex[:16]
        identifiers[original] = graphic["folioId"]
    for name, data in parts.items():
        if name.endswith(".xml"):
            for previous, stable in identifiers.items():
                data = data.replace(previous.encode(), stable.encode())
            parts[name] = data
    font_rows = []
    for ref in manifest["references"]:
        if ref["kind"] != "font":
            continue
        face = ref["font"]
        family, style = face["family"], face["style"]
        file = f"fonts/{family}-{style}.ttf"
        data = (SAMPLE/file).read_bytes()
        version = font_version(data)
        ps = next(ps for fam, face_style, ps in FACES.values() if (fam,face_style)==(family,style))
        ref.update(path=file, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), status="copied",
                   font=dict(family=family,style=style,postscript=ps,version=version,status="copied"))
        ref.pop("originalName", None)
        font_rows.append(dict(action="copy",family=family,style=style,version=version,file=file))
    if len(font_rows) != len(FACES):
        raise ValueError("Unexpected set of fonts in the issue")
    report.update(time=date, fonts=font_rows, fontCounts=dict(used=2,copy=2,record=0,missing=0,substituted=0))
    report.pop("limits", None)  # The engine also attaches a general development compatibility catalog.
    parts["folio/manifest.json"] = json_bytes(manifest)
    parts["folio/import-report.json"] = json_bytes(report)
    output.mkdir(parents=True, exist_ok=True)
    xml_zip(output/"november.folio", parts, parts.pop("mimetype").decode())
    (output/"folio.project").write_bytes(json_bytes(project))
    shutil.copytree(staging/"assets",output/"assets")


def build(args):
    output=args.output.resolve()
    if (output/"folio.project").exists() or output.exists() and any(output.iterdir()):
        raise ValueError("Choose an empty output directory")
    work=args.work.resolve()
    work.mkdir(parents=True,exist_ok=True)
    font_dir=SAMPLE/"fonts"
    with tempfile.TemporaryDirectory(dir=work) as temp:
        temp=Path(temp)
        write_assets(temp/"Links")
        composition().write(temp/"november.idml")
        profile=work/"userdata"
        env=dict(os.environ)
        for key,name in [("FOLIO_USER_DATA","app"),("FOLIO_CACHE_DIR","cache"),("FOLIO_COLOR_CACHE","color")]:
            folder=profile/name
            folder.mkdir(parents=True,exist_ok=True)
            env[key] = str(folder)
        staging = temp/"project"
        engine=Engine(args.engine.resolve(),env)
        try:
            engine.ask("projectCreate",dir=str(staging),name=PROJECT_NAME)
            engine.ask("importRun",source=str(temp/"november.idml"),project=str(staging),
                       document="november.folio",roots=[str(temp/"Links")],newDocument=True)
        finally:
            engine.close()
        finish_project(staging,output)
    shutil.copytree(font_dir,output/"fonts",dirs_exist_ok=True)
    for name in ("GRACoL2013_CRPC6.icc", "sRGB-v2-micro.icc", "PRINT-PROFILE.md"):
        shutil.copyfile(SAMPLE/name, output/name)
    assets=output/"assets"/"november"
    credits=["# 그림 제작 기록", "", "모든 그림은 이 호를 위해 새로 그렸다. 생성 프롬프트와 외부 원화는 없다.",
             "그림은 저장소의 라이선스로 배포한다. 각 SVG는 next/tools/samples/riverbend_art.py에서 다시 만들 수 있다.", ""]
    for name,(_,description) in ILLUSTRATIONS.items():
        credits.append(f"- {name}.svg — {description}")
    credits += ["", "11쪽의 막대 도표와 18쪽 광고 무늬는 riverbend_layout.py에서 편집 가능한 지면 도형으로 만들었다."]
    (assets/"CREDITS.md").write_bytes(("\n".join(credits)+"\n").encode())
    if (SAMPLE/"README.md").is_file():
        shutil.copyfile(SAMPLE/"README.md",output/"README.md")


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ("engine","output","work"):
        parser.add_argument("--"+key,type=Path,required=True)
    build(parser.parse_args())
