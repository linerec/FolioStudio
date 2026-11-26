"""Builds Folio Studio into a folder that runs without the source tree, and deploys it to a
synced folder (Synology Drive) so every computer gets the same build.

  python -I next/tools/package-app.py windows                 # build + assemble
  python -I next/tools/package-app.py windows --deploy DIR    # ... and mirror into DIR/Windows
  python -I next/tools/package-app.py mac --mac-arch all       # arm64 + Intel ZIPs, ad-hoc signed

Everything is built from the checked-out commit into a work folder outside the source tree
(FOLIO_PACKAGE_WORK, else %LOCALAPPDATA%/FolioStudio-build/package: its own cargo target, vite
output, download cache and the assembled app). FOLIO_PACKAGE_TARGET can select a dedicated Cargo
target directory for the Windows build. The running app (next/engine/target) is left alone.
A dirty work tree is refused unless --allow-dirty: a deployed build must match a commit.

DIR layout (only these names are written; nothing else in DIR is touched):
  Windows/                    the app (Folio Studio.exe + resources/app, engine, MCP, INDD reader)
  Folio Studio (Windows).cmd  launcher: copies Windows/ to %LOCALAPPDATA%/FolioStudio/app
                              and starts it there, so the synced copy is never locked
  BUILD.json                  commit, time, file hashes (written last)
  README.txt
"""
import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..'))
NEXT = os.path.join(REPO, 'next')
APP = os.path.join(NEXT, 'app')
WORK = os.environ.get('FOLIO_PACKAGE_WORK') or os.path.join(
    os.environ.get('LOCALAPPDATA') or os.path.join(os.path.expanduser('~'), '.cache'), 'FolioStudio-build', 'package')
TARGET = os.path.join(WORK, 'target')
NAME = 'Folio Studio'


def _compiler_crashed(cmd, p):
    """True when a cargo build died only because rustc itself crashed (exit code 0xc0000005 / 0xc0000409, no real rustc diagnostic).
    On the development PC (suspected CPU instability, 2026-11-02) this happened at random on code that built fine by hand."""
    if p.returncode == 0 or not isinstance(cmd, (list, tuple)) or not cmd or os.path.basename(str(cmd[0])).lower() not in ('cargo', 'cargo.exe'):
        return False
    out = (p.stdout or '') + (p.stderr or '')
    return bool(re.search(r'exit code: 0xc0000(005|409)', out)) and not re.search(r'^error\[E\d+\]', out, re.M)


def run(cmd, cwd=REPO, env=None):
    for attempt in (1, 2, 3):
        p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace', shell=isinstance(cmd, str))
        if attempt < 3 and _compiler_crashed(cmd, p):
            print(f'rustc crashed (attempt {attempt} of 3): running the build again', flush=True)
            continue
        break
    if p.returncode != 0:
        tail = (p.stdout + p.stderr).strip().splitlines()[-15:]
        raise SystemExit(f'failed: {cmd}\n' + '\n'.join(tail))
    return p.stdout


def git(*args):
    return run(['git', *args]).strip()


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for b in iter(lambda: f.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


def build_windows():
    """Release engine and INDD reader with the static MSVC runtime, UI into WORK/dist."""
    if not os.path.isfile(os.path.join(APP, 'node_modules', 'electron', 'dist', 'electron.exe')):
        run(['node', 'node_modules/electron/install.js'], cwd=APP)
    env = os.environ.copy()
    flags = 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS'
    env[flags] = (env.get(flags, '') + ' -C target-feature=+crt-static').strip()
    target_override = env.get('FOLIO_PACKAGE_TARGET')
    engine_target = target_override or os.path.join(TARGET, 'engine')
    reader_target = target_override or os.path.join(TARGET, 'reader')
    run(['cargo', 'build', '--release', '--manifest-path', os.path.join(NEXT, 'engine', 'Cargo.toml'),
         '-p', 'folio-engine', '--bin', 'folio-engine', '--bin', 'folio-mcp', '--target-dir', engine_target], env=env)
    run(['cargo', 'build', '--release', '--manifest-path', os.path.join(REPO, 'native', 'indd-reader', 'Cargo.toml'),
         '--target-dir', reader_target], env=env)
    dist = os.path.join(WORK, 'dist')
    run(f'npx vite build --outDir "{dist}" --emptyOutDir', cwd=APP)
    return {
        'engine': os.path.join(REPO, engine_target, 'release', 'folio-engine.exe'),
        'mcp': os.path.join(REPO, engine_target, 'release', 'folio-mcp.exe'),
        'reader': os.path.join(REPO, reader_target, 'release', 'folio-indd-reader.exe'),
        'dist': dist,
    }


def assemble_windows(built):
    out = os.path.join(WORK, 'windows', NAME)
    if os.path.isdir(out):
        shutil.rmtree(out)
    shutil.copytree(os.path.join(APP, 'node_modules', 'electron', 'dist'), out)
    os.replace(os.path.join(out, 'electron.exe'), os.path.join(out, NAME + '.exe'))
    res = os.path.join(out, 'resources')
    default_app = os.path.join(res, 'default_app.asar')
    if os.path.exists(default_app):
        os.remove(default_app)
    app = os.path.join(res, 'app')
    os.makedirs(os.path.join(app, 'electron'))
    pkg = json.load(open(os.path.join(APP, 'package.json'), encoding='utf-8'))
    # name stays as in development: the same settings folder (recent documents) on this machine
    json.dump({'name': pkg['name'], 'version': pkg['version'], 'main': pkg['main']},
              open(os.path.join(app, 'package.json'), 'w', encoding='utf-8'), indent=1)
    # every main-process module (main.cjs requires its siblings)
    for f in sorted(n for n in os.listdir(os.path.join(APP, 'electron')) if n.endswith('.cjs')):
        shutil.copy2(os.path.join(APP, 'electron', f), os.path.join(app, 'electron', f))
    shutil.copytree(built['dist'], os.path.join(app, 'dist'))
    # main.cjs looks for the engine in resources/, the engine for the reader beside itself
    shutil.copy2(built['engine'], os.path.join(res, 'folio-engine.exe'))
    shutil.copy2(built['mcp'], os.path.join(res, 'folio-mcp.exe'))
    shutil.copy2(built['reader'], os.path.join(res, 'folio-indd-reader.exe'))
    copy_sample(res)
    return out


def copy_sample(res):
    """Bundle the IDML atlas and the editable magazine with their relative assets."""
    for name in ('feature-atlas', 'riverbend-letters'):
        source = os.path.join(REPO, 'samples', name)
        shutil.copytree(source, os.path.join(res, 'samples', name),
                        ignore=shutil.ignore_patterns('.folio'))


def smoke(folder):
    """Starts the assembled app with its own settings folder and checks it draws a window."""
    shot = os.path.join(WORK, 'smoke-windows.png')
    if os.path.exists(shot):
        os.remove(shot)
    profile = os.environ.get('FOLIO_PACKAGE_SMOKE_USER_DATA') or os.path.join(WORK, 'smoke-user-data')
    env = {**os.environ, 'FOLIO_USER_DATA': profile, 'FOLIO_SCREENSHOT': shot,
           'FOLIO_SCREENSHOT_DELAY': '4000', 'FOLIO_SCREENSHOT_QUIT': '1'}
    env.pop('FOLIO_ENGINE', None)
    env.pop('FOLIO_DIST', None)
    subprocess.run([os.path.join(folder, NAME + '.exe')], env=env, timeout=90, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not os.path.exists(shot) or os.path.getsize(shot) < 10000:
        raise SystemExit('smoke: the packaged app did not draw its window')
    return shot


DEPLOYED = '.folio-deployed.json'
ASIDE = '.folio-old'


def is_aside(name):
    return name.rstrip('0123456789').endswith(ASIDE)


def replace_busy(tmp, d):
    """os.replace, but a file a running app holds open (on Windows the engine .exe or a DLL) cannot be
    overwritten, only renamed. The running app keeps the old file, the next start gets the new one, and
    the renamed file goes at a later deploy once that app is closed. (2026-10-25: a deploy stopped with
    Access is denied because the user's Folio app was open.)"""
    try:
        os.replace(tmp, d)
    except PermissionError:
        old, n = d + ASIDE, 1
        while os.path.exists(old):
            old, n = f'{d}{ASIDE}{n}', n + 1
        os.rename(d, old)
        os.replace(tmp, d)


def mirror(src, dst):
    """Copies src into dst and removes only files an earlier deploy put there.

    dst is a synced folder people use: anything this script did not write (an app someone
    unzipped there, a log they dropped in) is left alone and reported. The files each deploy
    wrote are listed in dst/.folio-deployed.json; a later deploy removes those it no longer
    produces. (2026-10-20: a mirror that removed every unknown file deleted a Mac user's
    unzipped Folio Studio.app from the Synology folder, and the deletion synced to the Mac.)
    """
    key = lambda rel: os.path.normcase(rel.replace('\\', '/'))
    record = os.path.join(dst, DEPLOYED)
    try:
        previous = {key(r) for r in json.load(open(record, encoding='utf-8'))['files']}
    except (OSError, ValueError, KeyError):
        previous = set()
    for root, dirs, files in os.walk(dst):  # files renamed aside by an earlier deploy; one a running app still holds stays
        for f in files:
            if is_aside(f):
                try:
                    os.remove(os.path.join(root, f))
                except OSError:
                    pass
    copied = removed = 0
    written = []
    for root, dirs, files in os.walk(src):
        rel = os.path.relpath(root, src)
        os.makedirs(os.path.join(dst, rel), exist_ok=True)
        for f in files:
            s, d = os.path.join(root, f), os.path.join(dst, rel, f)
            written.append(os.path.normpath(os.path.join(rel, f)).replace('\\', '/'))
            if os.path.exists(d) and os.path.getsize(d) == os.path.getsize(s) and sha256(d) == sha256(s):
                continue
            tmp = d + '.folio-new'
            shutil.copy2(s, tmp)
            replace_busy(tmp, d)
            copied += 1
    now = {key(r) for r in written}
    unknown = []
    for root, dirs, files in os.walk(dst, topdown=False):
        for f in files:
            p = os.path.join(root, f)
            rel = key(os.path.relpath(p, dst))
            if rel in now or rel == key(DEPLOYED) or is_aside(f):
                continue
            if rel in previous:
                os.remove(p)
                removed += 1
            else:
                unknown.append(os.path.relpath(p, dst))
        if root != dst and not os.listdir(root) and any(r.startswith(key(os.path.relpath(root, dst)) + '/') for r in previous):
            os.rmdir(root)
    tmp = record + '.folio-new'
    json.dump({'files': written}, open(tmp, 'w', encoding='utf-8'), indent=0, ensure_ascii=False)
    os.replace(tmp, record)
    if unknown:
        tops = sorted({u.replace('\\', '/').split('/')[0] for u in unknown})
        print(f'left alone in {dst} (not written by a deploy): {len(unknown)} files under {", ".join(tops[:5])}')
    return copied, removed


LAUNCHER = r'''@echo off
rem Folio Studio: copies the app from this synced folder to this computer and starts it there,
rem so Synology Drive can update the synced copy while the app is open.
setlocal
set "SRC=%~dp0Windows"
set "DST=%LOCALAPPDATA%\FolioStudio\app"
if not exist "%SRC%\Folio Studio.exe" goto wait_for_sync
if not exist "%SRC%\resources\folio-engine.exe" goto wait_for_sync
if not exist "%SRC%\resources\folio-mcp.exe" goto wait_for_sync
if not exist "%SRC%\resources\folio-indd-reader.exe" goto wait_for_sync
if not exist "%SRC%\resources\app\package.json" goto wait_for_sync
robocopy "%SRC%" "%DST%" /MIR /R:0 /W:0 /NFL /NDL /NJH /NJS /NP >nul
if %ERRORLEVEL% GEQ 8 echo Some files could not be updated, probably because Folio Studio is open. Close it and run this again to update.
start "" "%DST%\Folio Studio.exe" %*
exit /b

:wait_for_sync
echo Folio Studio is not here yet. Wait until Synology Drive finishes syncing this folder.
pause
exit /b 1
'''

README = '''Folio Studio
============

이 폴더는 시놀로지 드라이브로 모든 컴퓨터에 똑같이 동기화됩니다. 새 빌드가 나오면 여기 내용이 바뀝니다.
지금 들어 있는 빌드: BUILD.json (커밋, 시각)

Windows
- 다른 컴퓨터에서 처음 실행할 때는 동기화가 끝난 뒤 "Folio Studio (Windows).cmd"를 두 번 누릅니다. Windows 폴더의 exe를 직접 열 수도 있지만, 그렇게 열어 둔 동안에는 동기화가 막힙니다.
- 앱을 이 컴퓨터의 %LOCALAPPDATA%\\FolioStudio\\app 로 복사한 뒤 거기서 띄웁니다. 그래서 앱이 열려 있어도 동기화가 막히지 않습니다.
- 새 빌드는 앱을 닫고 다시 실행하면 적용됩니다.

Mac
- Mac/README.txt를 따릅니다(Apple Silicon). Mac/Folio Studio.zip을 다운로드 폴더로 복사한 뒤 풀고, 응용 프로그램 폴더로 옮깁니다.
- 이 폴더 안에서 zip을 풀지 마세요. 풀린 앱이 모든 컴퓨터로 동기화됩니다.

기능 표본 (개발 중 확인용)
- 시작 화면의 "기능 표본 열기 (Atlas)" 또는 도움말 ▸ 기능 표본 열기를 누르면 Feature_Atlas.idml(51쪽, 표본 181개)가 바로 열립니다.
- 그 컴퓨터의 문서 폴더 안 Folio Studio ▸ Feature Atlas 프로젝트에 놓입니다. 앱이 새 빌드로 바뀌면 다시 가져와 최신 결과를 보여 줍니다.
- 표본을 고쳐서 저장했다면 그 파일은 그대로 두고, 새로 가져온 것은 날짜가 붙은 이름이 됩니다.

문서와 프로젝트는 이 폴더에 두지 마세요. 이 폴더는 빌드할 때마다 새로 맞춰집니다.
'''


def deploy(folder, to, info):
    to = os.path.abspath(to)
    os.makedirs(to, exist_ok=True)
    copied, removed = mirror(folder, os.path.join(to, 'Windows'))
    with open(os.path.join(to, NAME + ' (Windows).cmd'), 'w', encoding='ascii', newline='\r\n') as f:
        f.write(LAUNCHER)
    with open(os.path.join(to, 'README.txt'), 'w', encoding='utf-8-sig', newline='\r\n') as f:
        f.write(README)
    build_json = os.path.join(to, 'BUILD.json')
    old = json.load(open(build_json, encoding='utf-8')) if os.path.exists(build_json) else {}
    old.update(info)
    tmp = build_json + '.folio-new'
    json.dump(old, open(tmp, 'w', encoding='utf-8'), indent=1, ensure_ascii=False)
    os.replace(tmp, build_json)  # last: a computer that sees this sees a whole build
    return copied, removed


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('platform', choices=['windows', 'mac'])
    ap.add_argument('--mac-arch', choices=['arm64', 'x64', 'all'], default='arm64')
    ap.add_argument('--deploy', help='destination folder chosen by the user')
    ap.add_argument('--allow-dirty', action='store_true')
    a = ap.parse_args()
    dirty = git('status', '--porcelain', '--untracked-files=no')
    if dirty and not a.allow_dirty:
        raise SystemExit('work tree has uncommitted changes; commit first or pass --allow-dirty')
    commit = git('rev-parse', '--short', 'HEAD')
    with open(os.path.join(APP, 'package.json'), encoding='utf-8') as manifest:
        version = json.load(manifest)['version']
    # -I excludes the script directory from sys.path; load only our sibling modules.
    sys.path.insert(0, HERE)
    if a.platform == 'mac':
        import package_mac
        package_mac.main(sys.modules[__name__], a, {
            'version': version,
            'commit': commit + ('+dirty' if dirty else ''),
            'subject': git('log', '-1', '--format=%s'),
            'built': datetime.datetime.now().astimezone().isoformat(timespec='seconds'),
        })
        return
    from windows_pe import dependency_errors

    built = build_windows()
    folder = assemble_windows(built)
    errors = dependency_errors(folder)
    if errors:
        raise SystemExit('Windows package has unresolved DLL imports:\n' + '\n'.join(errors))
    shot = smoke(folder)
    info = {'windows': {
        'version': version,
        'commit': commit + ('+dirty' if dirty else ''),
        'subject': git('log', '-1', '--format=%s'),
        'built': datetime.datetime.now().astimezone().isoformat(timespec='seconds'),
        'engine': sha256(os.path.join(folder, 'resources', 'folio-engine.exe')),
        'mcp': sha256(os.path.join(folder, 'resources', 'folio-mcp.exe')),
        'reader': sha256(os.path.join(folder, 'resources', 'folio-indd-reader.exe')),
    }}
    print(f'assembled {folder} ({commit}); smoke {shot}')
    if a.deploy:
        copied, removed = deploy(folder, a.deploy, info)
        print(f'deployed to {a.deploy}: {copied} files copied, {removed} removed')


if __name__ == '__main__':
    main()
