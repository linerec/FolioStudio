"""Windows-hosted Mac packaging. Downloaded archives are data, never installer scripts.

Keep this separate so the Windows builder's assembly/deployment stays unchanged.
Sources: github.com/electron/electron/releases and
github.com/indygreg/apple-platform-rs/releases (rcodesign 0.29.0).
"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import shutil
import stat
import struct
import subprocess
import sys
import urllib.request
import zipfile
from mac_signatures import signature, verify_code, verify_resources
from tool_support import read_json, read_log

NAME = 'Folio Studio'
BUNDLE_ID = 'studio.folio.desktop'
SIGN_VERSION = '0.29.0'
MACH_MAGIC = {b'\xcf\xfa\xed\xfe', b'\xfe\xed\xfa\xcf', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca'}
ARCHES = {'arm64': ('aarch64-apple-darwin', 0x100000c), 'x64': ('x86_64-apple-darwin', 0x1000007)}


def digest(path):
    with open(path, 'rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def download(url, dest):
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if not dest.exists():
        tmp = dest.with_suffix(dest.suffix + '.partial')
        with urllib.request.urlopen(url, timeout=120) as src, open(tmp, 'wb') as out:
            shutil.copyfileobj(src, out)
        tmp.replace(dest)
    return dest


def verified_download(base, name, sums, cache):
    expected = next((line.split()[0] for line in sums.splitlines()
                     if len(line.split()) >= 2 and line.split()[-1].lstrip('*') == name), None)
    if not expected:
        raise RuntimeError(f'No official SHA-256 for {name}')
    path = download(base + name, Path(cache) / name)
    if digest(path) != expected.lower():
        raise RuntimeError(f'SHA-256 mismatch: {path}; remove this cache file and retry')
    return path


def rcodesign(p):
    base = f'https://github.com/indygreg/apple-platform-rs/releases/download/apple-codesign/{SIGN_VERSION}/'
    name = f'apple-codesign-{SIGN_VERSION}-x86_64-pc-windows-msvc.zip'
    cache = Path(p.WORK) / 'downloads'
    sums = download(base + name + '.sha256', cache / (name + '.sha256')).read_text()
    # Release sidecars contain just the digest on some versions.
    expected = sums.split()[0]
    archive = verified_download(base, name, f'{expected}  {name}', cache)
    tool = Path(p.WORK) / 'tools' / f'rcodesign-{SIGN_VERSION}' / 'rcodesign.exe'
    with zipfile.ZipFile(archive) as z:
        entry = next(n for n in z.namelist() if n.endswith('/rcodesign.exe') or n == 'rcodesign.exe')
        binary = z.read(entry)
    tool.parent.mkdir(parents=True, exist_ok=True)
    if not tool.exists() or tool.read_bytes() != binary:
        tool.write_bytes(binary)
    return tool


def electron_archive(p, arch):
    version = json.loads((Path(p.APP) / 'node_modules/electron/package.json').read_text())['version']
    base = f'https://github.com/electron/electron/releases/download/v{version}/'
    cache = Path(p.WORK) / 'downloads'
    sums = download(base + 'SHASUMS256.txt', cache / f'electron-v{version}-SHASUMS256.txt').read_text()
    name = f'electron-v{version}-darwin-{arch}.zip'
    return verified_download(base, name, sums, cache), version


def inside(root, path):
    return path.resolve().is_relative_to(root.resolve())


def extract_electron(archive, stage):
    """Real links for rcodesign on NTFS; ZIP modes are restored explicitly at output.

    Requires unprivileged symlink support (Windows Developer Mode). Never falls back to
    regular copies: signing copied framework links would produce an invalid Mac bundle.
    """
    stage = Path(stage)
    stage.mkdir(parents=True, exist_ok=True)
    links = []
    with zipfile.ZipFile(archive) as z:
        for entry in z.infolist():
            parts = PurePosixPath(entry.filename).parts
            if not parts or parts[0] != 'Electron.app':
                continue
            if '..' in parts or '\\' in entry.filename or any(':' in c for c in parts):
                raise RuntimeError(f'Unsafe archive name: {entry.filename}')
            dest = stage.joinpath(*parts)
            if not inside(stage, dest):
                raise RuntimeError(f'Archive escapes staging: {entry.filename}')
            if stat.S_ISLNK(entry.external_attr >> 16):
                links.append((dest, z.read(entry).decode('utf-8')))
            elif entry.is_dir():
                dest.mkdir(parents=True, exist_ok=True)
            else:
                dest.parent.mkdir(parents=True, exist_ok=True)
                with z.open(entry) as src, open(dest, 'wb') as out:
                    shutil.copyfileobj(src, out)
        # Versions/Current must exist before links through it, for Windows directory typing.
        links.sort(key=lambda item: (item[0].name != 'Current', str(item[0])))
        for dest, target in links:
            if PurePosixPath(target).is_absolute() or '\\' in target or ':' in target:
                raise RuntimeError(f'Unsafe link: {dest} -> {target}')
            resolved = dest.parent.joinpath(*PurePosixPath(target).parts)
            if not inside(stage / 'Electron.app', resolved) or not resolved.exists():
                raise RuntimeError(f'Broken/escaping link: {dest} -> {target}')
            dest.parent.mkdir(parents=True, exist_ok=True)
            # NTFS relative links require native separators. rcodesign and make_zip
            # normalize them back to '/' in the resource seal and ZIP link payload.
            os.symlink(target.replace('/', os.sep), dest, target_is_directory=resolved.is_dir())
    return stage / 'Electron.app'


def run_logged(p, args, name, env=None, cwd=None):
    log = Path(p.WORK) / 'logs' / name
    log.parent.mkdir(parents=True, exist_ok=True)
    for attempt in (1, 2, 3):
        with open(log, 'w', encoding='utf-8') as f:
            # in the work folder: zig leaves .intentionally-empty-file.o where it runs
            os.makedirs(p.WORK, exist_ok=True)
            result = subprocess.run([str(a) for a in args], cwd=cwd or p.WORK, env=env,
                                    stdout=f, stderr=subprocess.STDOUT)
        # rustc dies at random on the development PC (exit code 0xc0000005 / 0xc0000409, no compile error;
        # 2026-11-02, see artifacts/rustc-crash-20261102/FINDINGS.ko.md): run the build again, up to 3 times
        if result.returncode and attempt < 3 and str(args[0]).lower().endswith(('cargo', 'cargo.exe')):
            text = read_log(log)
            if re.search(r'exit code: 0xc0000(005|409)', text) and not re.search(r'^error\[E\d+\]', text, re.M):
                print(f'rustc crashed (attempt {attempt} of 3): running {name} again', flush=True)
                continue
        break
    if result.returncode:
        tail = '\n'.join(read_log(log).splitlines()[-12:])
        raise RuntimeError(f'Failed ({result.returncode}); {log}\n{tail}')
    return log


def build(p, arch):
    target, _ = ARCHES[arch]
    env = {**os.environ, 'CARGO_ZIGBUILD_PYTHON_PATH': sys.executable,
           'MACOSX_DEPLOYMENT_TARGET': '13.0'}
    if arch == 'x64':
        # Unlike arm64, Zig's Intel output starts unsigned. Reserve load-command
        # space for rcodesign's LC_CODE_SIGNATURE without moving executable code.
        key = 'CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS'
        env[key] = (env.get(key, '') + ' -C link-arg=-Wl,-headerpad,0x4000').strip()
    target_dir = Path(os.environ.get('FOLIO_PACKAGE_TARGET') or Path(p.NEXT) / 'engine/target')
    reader_dir = target_dir / 'mac-reader'
    print(f'building {target}', flush=True)
    run_logged(p, ['cargo', 'zigbuild', '--release', '--locked', '--manifest-path',
                  Path(p.NEXT) / 'engine/Cargo.toml', '-p', 'folio-engine',
                  '--bin', 'folio-engine', '--bin', 'folio-mcp',
                  '--target', target, '--target-dir', target_dir], f'mac-engine-{arch}.log', env)
    run_logged(p, ['cargo', 'zigbuild', '--release', '--locked', '--manifest-path',
                  Path(p.REPO) / 'native/indd-reader/Cargo.toml',
                  '--target', target, '--target-dir', reader_dir], f'mac-reader-{arch}.log', env)
    return {'engine': target_dir / target / 'release/folio-engine',
            'mcp': target_dir / target / 'release/folio-mcp',
            'reader': reader_dir / target / 'release/folio-indd-reader'}


def rewrite_plist(file, **updates):
    data = plistlib.loads(file.read_bytes())
    data.update(updates)
    data.pop('ElectronAsarIntegrity', None)  # default_app.asar has been removed
    file.write_bytes(plistlib.dumps(data, sort_keys=False))
    return data


def macho(path):
    with open(path, 'rb') as f:
        return f.read(4) in MACH_MAGIC


def regular_files(root):
    for folder, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = sorted(d for d in dirs if not (Path(folder) / d).is_symlink())
        for name in sorted(files):
            path = Path(folder) / name
            if not path.is_symlink():
                yield path


def assemble(p, arch, built, dist, tool):
    archive, version = electron_archive(p, arch)
    stage = Path(p.WORK) / 'mac-stage' / arch
    # All destructive cleanup is constrained to this worktree's package staging folder.
    if not inside(Path(p.WORK) / 'mac-stage', stage) or stage.is_symlink():
        raise RuntimeError('Unexpected staging path')
    if stage.exists():
        shutil.rmtree(stage)
    app = extract_electron(archive, stage)
    app = app.rename(stage / f'{NAME}.app')
    res = app / 'Contents/Resources'
    (res / 'default_app.asar').unlink(missing_ok=True)
    with zipfile.ZipFile(archive) as z:
        for license_file in ('LICENSE', 'LICENSES.chromium.html'):
            (res / license_file).write_bytes(z.read(license_file))
    pkg = read_json(Path(p.APP) / 'package.json')
    data = rewrite_plist(app / 'Contents/Info.plist', CFBundleName=NAME, CFBundleDisplayName=NAME,
                        CFBundleExecutable=NAME, CFBundleIdentifier=BUNDLE_ID,
                        CFBundleShortVersionString=pkg['version'], CFBundleVersion=pkg['version'],
                        LSApplicationCategoryType='public.app-category.graphics-design')
    (app / 'Contents/MacOS/Electron').rename(app / 'Contents/MacOS' / NAME)
    helpers = []
    for helper in sorted((app / 'Contents/Frameworks').glob('Electron Helper*.app')):
        old = helper.stem
        new = old.replace('Electron', NAME, 1)
        info = plistlib.loads((helper / 'Contents/Info.plist').read_bytes())
        suffix = info['CFBundleIdentifier'].removeprefix('com.github.Electron')
        rewrite_plist(helper / 'Contents/Info.plist', CFBundleName=new, CFBundleDisplayName=new,
                      CFBundleExecutable=new, CFBundleIdentifier=BUNDLE_ID + suffix)
        (helper / 'Contents/MacOS' / old).rename(helper / 'Contents/MacOS' / new)
        helpers.append(helper.rename(helper.with_name(new + '.app')))
    appdir = res / 'app'
    (appdir / 'electron').mkdir(parents=True)
    (appdir / 'package.json').write_text(json.dumps({k: pkg[k] for k in ('name', 'version', 'main')}, indent=1), encoding='utf-8')
    # every main-process module (main.cjs requires its siblings)
    for name in sorted(n.name for n in (Path(p.APP) / 'electron').glob('*.cjs')):
        shutil.copy2(Path(p.APP) / 'electron' / name, appdir / 'electron' / name)
    shutil.copytree(dist, appdir / 'dist')
    for key, dest in [('engine', 'folio-engine'), ('mcp', 'folio-mcp'), ('reader', 'folio-indd-reader')]:
        shutil.copy2(built[key], res / dest)
    p.copy_sample(res)
    # No fonts, ICC files, Adobe assets, or machine-specific project paths are bundled.
    entitlements = stage / 'entitlements.plist'
    entitlements.write_bytes(plistlib.dumps({
        'com.apple.security.cs.allow-jit': True,
        'com.apple.security.cs.allow-unsigned-executable-memory': True,
        'com.apple.security.cs.disable-library-validation': True,
    }))
    # relative to the folder the signer runs in (run_logged: the work folder): rcodesign's
    # helper:path form cannot take a drive letter's colon
    ent_rel = entitlements.relative_to(Path(p.WORK)).as_posix()
    args = [tool, 'sign', '--timestamp-url', 'none', '--entitlements-xml-file', ent_rel]
    for helper in helpers:
        args += ['--entitlements-xml-file', f'{helper.relative_to(app)}:{ent_rel}']
    args.append(app)
    run_logged(p, args, f'mac-sign-{arch}.log')
    return app, {'electron': version, 'minimumMacOS': data['LSMinimumSystemVersion'],
                 'electronArchive': digest(archive)}


def verify_macho(p, app, arch, tool):
    """Check architecture and signature of every real Mach-O, including helpers/dylibs."""
    expected = ARCHES[arch][1]
    result = []
    for path in regular_files(app):
        if not macho(path):
            continue
        with open(path, 'rb') as f:
            head = f.read(8)
        if head[:4] != b'\xcf\xfa\xed\xfe' or struct.unpack('<I', head[4:])[0] != expected:
            raise RuntimeError(f'Unexpected Mach-O architecture: {path}')
        relative = path.relative_to(app).as_posix()
        # rcodesign 0.29 verify rejects the intentionally empty ad-hoc CMS blob.
        # Its own warning says to use Apple's verifier. Inspect with its supported
        # print command, and independently check every signed code page below.
        log = run_logged(p, [tool, 'print-signature-info', path], f'mac-verify-{arch}-{len(result):02}.log')
        text = read_log(log)
        if 'CodeSignatureFlags(ADHOC)' not in text:
            raise RuntimeError(f'Signature verification needs attention: {log}')
        verify_code(path)
        if '/MacOS/' in '/' + relative:
            blobs = signature(path)[2]
            entitlements = plistlib.loads(blobs[5][8:]) if 5 in blobs else {}
            if not entitlements.get('com.apple.security.cs.allow-jit'):
                raise RuntimeError(f'Missing Electron JIT entitlement: {relative}')
        result.append({'path': relative, 'arch': arch, 'sha256': digest(path)})
    if len(result) < 10:
        raise RuntimeError('Too few Mach-O binaries in Electron bundle')
    verify_resources(app)
    return result


def make_zip(app, dest):
    """Unix attributes are explicit: Windows and Synology filesystem modes are irrelevant."""
    with zipfile.ZipFile(dest, 'w', zipfile.ZIP_DEFLATED, compresslevel=6, allowZip64=True) as z:
        root = zipfile.ZipInfo(app.name + '/')
        root.create_system = 3
        root.external_attr = (stat.S_IFDIR | 0o755) << 16
        z.writestr(root, b'')
        for folder, dirs, files in os.walk(app, followlinks=False):
            for name in sorted(dirs + files):
                path = Path(folder) / name
                arc = path.relative_to(app.parent).as_posix()
                if path.is_symlink():
                    mode = stat.S_IFLNK | 0o777
                    content = os.readlink(path).replace('\\', '/').encode('utf-8')
                elif path.is_dir():
                    mode = stat.S_IFDIR | 0o755
                    arc += '/'
                    content = b''
                else:
                    mode = stat.S_IFREG | (0o755 if macho(path) else 0o644)
                    content = None
                info = zipfile.ZipInfo(arc)
                info.create_system = 3
                info.external_attr = mode << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                if content is None:
                    with z.open(info, 'w', force_zip64=True) as out, open(path, 'rb') as src:
                        shutil.copyfileobj(src, out)
                else:
                    z.writestr(info, content)


def verify_zip(archive, machos):
    with zipfile.ZipFile(archive) as z:
        names = set(z.namelist())
        links = 0
        for entry in z.infolist():
            mode = entry.external_attr >> 16
            if entry.create_system != 3:
                raise RuntimeError('ZIP is missing Unix attributes')
            if stat.S_ISLNK(mode):
                links += 1
                target = z.read(entry).decode('utf-8')
                if target.startswith('/') or '\\' in target or ':' in target:
                    raise RuntimeError(f'Invalid ZIP link {entry.filename}')
        if links != 14:
            raise RuntimeError(f'Expected 14 Electron framework links, found {links}')
        for binary in machos:
            name = f'{NAME}.app/{binary["path"]}'
            entry = z.getinfo(name)
            if not (entry.external_attr >> 16) & 0o111:
                raise RuntimeError(f'Missing executable permission: {name}')
            if hashlib.sha256(z.read(name)).hexdigest() != binary['sha256']:
                raise RuntimeError(f'Binary changed during ZIP creation: {name}')
        if f'{NAME}.app/Contents/_CodeSignature/CodeResources' not in names:
            raise RuntimeError('Missing bundle signature resource seal')
    return {'symlinks': links, 'executables': len(machos)}


README = '''Folio Studio — Mac 시험판
========================

macOS {minimum} 이상. Apple 개발자 서명·공증 없이 ad-hoc 서명한 시험판입니다.
Windows에서 교차 빌드하고 서명·아키텍처·압축 속성을 검사했습니다. 실제 Mac 실행은 아직 확인하지 못했습니다.

설치
1. 시놀로지 동기화가 끝난 다음 이 파일을 Mac의 다운로드 폴더로 복사합니다.
   이 시놀로지 폴더 안에서 바로 풀지 마세요. 그러면 앱 파일 수천 개가 모든 컴퓨터로 동기화됩니다.
   Apple Silicon(M1/M2/M3/M4 등): Folio Studio.zip
   Intel Mac: Folio Studio Intel.zip (함께 있는 경우)
   기종은 Apple 메뉴 > 이 Mac에 관하여에서 확인합니다.
2. Mac Finder에서 zip을 두 번 눌러 풉니다. Windows에서 먼저 풀거나 .app 자체를 동기화하지 마세요.
   zip 안에 실행 권한과 프레임워크 심볼릭 링크가 들어 있습니다.
3. Folio Studio.app을 응용 프로그램 폴더로 옮겨 엽니다. 업데이트할 때는 앱을 종료하고 교체합니다.
   프로젝트 폴더는 앱과 별개이며 NAS·Mac·Windows로 통째로 옮겨도 참조를 유지합니다.

처음 열 때 차단되면
- 시스템 설정 > 개인정보 보호 및 보안 > 차단된 Folio Studio의 '확인 없이 열기'를 사용합니다.
- 본인이 받은 이 시험판을 신뢰하는 경우에만 터미널에서 아래 명령으로 이 앱의 격리 표시를 지웁니다.
  xattr -cr "/Applications/Folio Studio.app"
  open "/Applications/Folio Studio.app"
- 보안 기능을 시스템 전체에서 끄지 마세요. '지원하지 않는 앱'이면 Mac 기종과 zip 종류, macOS 버전을 확인합니다.

진단
앱이 시작될 때 엔진 자기 점검(OS·CPU·글꼴 수·프로필 수·캐시 쓰기·리더 실행)과 오류를 기록합니다.
로그 폴더: ~/Library/Logs/Folio Studio/
현재 실행: app.log / 직전 실행: app.previous.log
앱 도움말 메뉴 > 진단 로그 폴더 열기로도 볼 수 있습니다.
앱 설정: ~/Library/Application Support/folio-studio-next/
엔진 캐시: ~/Library/Caches/FolioStudioNext/

터미널에서 서명 확인:
  codesign --verify --deep --strict --verbose=2 "/Applications/Folio Studio.app"
서명이 틀리면 다시 동기화한 zip을 새로 풀어 확인하세요. 임의로 재서명하지 마세요.

앱이 로그를 남기기도 전에 실패할 때:
  mkdir -p "$HOME/Library/Logs/Folio Studio"
  "/Applications/Folio Studio.app/Contents/MacOS/Folio Studio" > "$HOME/Library/Logs/Folio Studio/launch.log" 2>&1
실행 후 터미널이 기다리는 것은 정상입니다. 앱에서 Command+Q로 종료합니다.

결과 전달
앱을 종료한 뒤 Finder > 이동 > 폴더로 이동에서 ~/Library/Logs/Folio Studio/ 를 엽니다.
로그 폴더를 압축하여 시놀로지 Drive의 Work 폴더 안 별도 'Folio Studio Mac 진단' 폴더에 넣습니다.
BUILD.json, Mac 기종, macOS 버전, 서명 확인 결과, 시도한 작업을 함께 적고 Claude에게 위치를 알려주세요.
로그에는 열었던 문서의 경로가 포함될 수 있으니 공유 전에 확인합니다.
앱 배포 폴더의 Mac 안에는 로그나 문서를 넣지 마세요(다음 배치 때 교체됩니다).

확인할 작업
- 시작 화면, Command 단축키·Option 끌기, 창 닫기 후 Dock으로 다시 열기, Command+Q.
- INDD 읽기, 글꼴·색 경고, PDF 출력, Windows에서 만든 프로젝트를 Mac으로 복사하여 열기.
- Mac에서 저장한 프로젝트 폴더 전체를 Windows로 복사하여 링크·그림·편집 내용을 확인하기.
- 글 입력: Option+좌우 단어 이동, Command+좌우 줄 처음·끝, Command+위아래 기사 처음·끝, Shift로 선택 확장.
- Edit > Preferences > Language(편집 > 환경 설정 > 언어)에서 한국어·English·OS 언어를 고릅니다.
  메뉴·시작 화면·상태 표시줄·대화상자·미완성 목록이 즉시 바뀝니다. 패널·컨트롤 번역은 후속 작업입니다.
- 한글 파일·폴더 이름이 NFC/NFD로 섞인 프로젝트를 옮겨 열고 그림·글꼴·저장·IDML 링크를 확인합니다.
'''


def main(p, args, info_base):
    arches = ['arm64', 'x64'] if args.mac_arch == 'all' else [args.mac_arch]
    tool = rcodesign(p)
    out = Path(p.WORK) / 'mac'
    out.mkdir(parents=True, exist_ok=True)
    for arch in ARCHES:
        if arch not in arches:
            stale = f'{NAME}.zip' if arch == 'arm64' else f'{NAME} Intel.zip'
            (out / stale).unlink(missing_ok=True)
    dist = Path(p.WORK) / 'dist-mac'
    # Use the installed Vite directly; no npm install or npx network fallback.
    run_logged(p, ['node', Path(p.APP) / 'node_modules/vite/bin/vite.js', 'build',
                  '--outDir', dist, '--emptyOutDir'], 'mac-ui.log', cwd=p.APP)
    builds = {}
    for arch in arches:
        built = build(p, arch)
        app, details = assemble(p, arch, built, dist, tool)
        binaries = verify_macho(p, app, arch, tool)
        zip_name = f'{NAME}.zip' if arch == 'arm64' else f'{NAME} Intel.zip'
        archive = out / zip_name
        make_zip(app, archive)
        checked = verify_zip(archive, binaries)
        builds[arch] = {**details, 'file': zip_name, 'sha256': digest(archive),
                       'bytes': archive.stat().st_size, 'engine': digest(app / 'Contents/Resources/folio-engine'),
                       'mcp': digest(app / 'Contents/Resources/folio-mcp'),
                       'reader': digest(app / 'Contents/Resources/folio-indd-reader'), **checked}
        (Path(p.WORK) / 'logs' / f'mac-machos-{arch}.json').write_text(json.dumps(binaries, indent=1), encoding='utf-8')
        print(f'verified {archive}: {archive.stat().st_size:,} bytes; {checked}', flush=True)
    minimum = next(iter(builds.values()))['minimumMacOS']
    readme = README.format(minimum=minimum)
    pkg_name = read_json(Path(p.APP) / 'package.json')['name']
    readme = readme.replace('Application Support/folio-studio-next/', f'Application Support/{pkg_name}/')
    (out / 'README.txt').write_text(readme, encoding='utf-8-sig')
    primary = builds.get('arm64', next(iter(builds.values())))
    info = {'mac': {**info_base, 'engine': primary['engine'], 'mcp': primary['mcp'], 'reader': primary['reader'],
                    'signing': 'ad-hoc; not notarized', 'runtimeTested': False, 'architectures': builds}}
    manifest = Path(p.WORK) / 'BUILD.json'
    update_manifest(manifest, info)
    if args.deploy:
        to = Path(args.deploy).absolute()
        to.mkdir(parents=True, exist_ok=True)
        # Publish delivery files only. Never synchronize the NTFS .app staging tree.
        copied, removed = p.mirror(str(out), str(to / 'Mac'))
        update_manifest(to / 'BUILD.json', info)
        print(f'deployed Mac to {to}: {copied} copied, {removed} removed')
    return info


def update_manifest(path, info):
    old = read_json(path) if path.exists() else {}
    old.update(info)  # preserve Windows metadata byte-for-byte as JSON values
    tmp = path.with_suffix('.json.folio-new')
    tmp.write_text(json.dumps(old, ensure_ascii=False, indent=1), encoding='utf-8')
    tmp.replace(path)
