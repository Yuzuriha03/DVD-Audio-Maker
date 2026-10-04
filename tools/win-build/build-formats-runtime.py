"""Build the small in-process C17 format runtime for Windows x64."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent / "native"))
from pe_dependencies import Pe

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--msys-root", type=Path, required=True)
parser.add_argument("--output", type=Path, default=Path("build/formats-native"))
args = parser.parse_args()

repo = Path(__file__).resolve().parents[2]
compiler = args.msys_root.resolve() / "mingw64/bin"
source = repo / "tools/formats-native/dvda-formats.c"
header = repo / "tools/formats-native/dvda-formats.h"
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
if not (compiler / "gcc.exe").is_file():
    raise FileNotFoundError(f"MSYS2 MinGW GCC not found: {compiler / 'gcc.exe'}")

subprocess.run([
    str(compiler / "gcc.exe"), "-std=c17", "-O2", "-Wall", "-Wextra", "-Werror",
    "-shared", "-static-libgcc", "-s", "-Wl,--no-insert-timestamp",
    str(source), "-o", str(output / "dvda-formats.dll"),
], env=os.environ | {"PATH": str(compiler) + os.pathsep + os.environ.get("PATH", "")}, check=True)

library = output / "dvda-formats.dll"
if Pe(library).machine != 0x8664:
    raise RuntimeError("dvda-formats.dll is not Windows x64")
imports = sorted(Pe(library).imports())
allowed = {"KERNEL32.dll", "kernel32.dll", "msvcrt.dll", "MSVCRT.dll"}
unexpected = [name for name in imports if name not in allowed]
if unexpected:
    raise RuntimeError("Unexpected format runtime imports: " + ", ".join(unexpected))

record = {
    "profile": "c17-formats-runtime",
    "compiler": str(compiler / "gcc.exe"),
    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "header_sha256": hashlib.sha256(header.read_bytes()).hexdigest(),
    "files": {
        library.name: {
            "bytes": library.stat().st_size,
            "sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
            "imports": imports,
        }
    },
}
(output / "formats-build.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
print(json.dumps(record, indent=2))
