"""Shared paths, input readers, and test environment for developer tools."""
import json
import os
from pathlib import Path
import tempfile


def release_engine_path(repo, target='target', *, binary=None):
    if binary is None:
        binary = 'folio-engine.exe' if os.name == 'nt' else 'folio-engine'
    return os.path.join(repo, 'next', 'engine', target, 'release', binary)


def profile_env(user_data, *, cache=None, color_cache=None):
    paths = {
        'FOLIO_USER_DATA': user_data,
        'FOLIO_CACHE_DIR': cache,
        'FOLIO_COLOR_CACHE': color_cache,
    }
    return {key: str(path) for key, path in paths.items() if path is not None}


def isolated_env(output=None, base=None, user_data=None):
    output = Path(output) if output is not None else Path(__file__).resolve().parents[2] / 'artifacts/oracle'
    override = (os.environ if base is None else base).get('FOLIO_ORACLE_PROFILE_ROOT')
    profiles = Path(override).resolve() if override else output.resolve() / 'userdata'
    profiles.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='run-', dir=profiles))
    env = dict(os.environ if base is None else base)
    # LOCALAPPDATA / APPDATA stay as they are: the user font folders (Microsoft/Windows/Fonts, Adobe
    # CoreSync) and the colour profiles live under them, and a test without them composes #347 with
    # 47 missing fonts (2026-11-19). Folio's own state moves through FOLIO_USER_DATA / FOLIO_CACHE_DIR.
    app = Path(user_data).resolve() if user_data is not None else root / 'app'
    cache = root / 'cache'
    color_cache = cache / 'color-cache'
    for path in (app, cache, color_cache):
        path.mkdir(parents=True, exist_ok=True)
    env.update(profile_env(app, cache=cache, color_cache=color_cache))
    return env


def read_json(path, *, encoding='utf-8'):
    return json.loads(Path(path).read_text(encoding=encoding))


def read_log(path):
    return Path(path).read_text(encoding='utf-8', errors='replace')


def indesign_jsx_command(runner, script, result, config):
    """Build the arguments for the caller's existing InDesign runner."""
    return ['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(runner),
            '-Script', str(script), '-Result', str(result), '-Config', json.dumps(config)]
