"""Run the A7 comparison with Cargo's observer executable."""

import importlib.util
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys


def main(profile):
    root = Path(__file__).resolve().parents[5]
    engine = root / "next/engine"
    isolated_env = runpy.run_path(str(root / "next/oracle/test_env.py"))["isolated_env"]
    env = isolated_env()
    env.setdefault("CARGO_TARGET_DIR", str(engine / "target-edit"))
    command = [
        os.environ["CARGO"], "build", "--locked", "--message-format=json",
        "-p", "folio-render", "--features", "vector-tools", "--example", "a7-observe",
    ]
    if profile == "release":
        command.append("--release")
    build = subprocess.run(
        command, cwd=engine, env=env, check=True, stdout=subprocess.PIPE, encoding="utf-8",
    )
    for line in build.stdout.splitlines():
        record = json.loads(line)
        if (record["reason"] == "compiler-artifact"
                and record["target"]["name"] == "a7-observe"
                and record["executable"]):
            exe = record["executable"]
            break
    else:
        raise RuntimeError("Cargo did not report the A7 observer executable")

    spec = importlib.util.spec_from_file_location("a7_check", root / "cleanroom/vectors/A7/check.py")
    checker = importlib.util.module_from_spec(spec)
    sys.dont_write_bytecode = True
    spec.loader.exec_module(checker)
    # Reuse the standalone checker's comparison with Cargo handling the linking.
    checker.build_harness = lambda: (exe, env)
    return checker.main(["--quiet"])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
