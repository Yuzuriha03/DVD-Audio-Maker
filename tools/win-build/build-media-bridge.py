"""Build and collect the in-process Windows x64 media runtime."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, sys
sys.path.insert(0, str(Path(__file__).resolve().parent / 'native'))
from pe_dependencies import Pe

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--msys-root', type=Path, required=True)
parser.add_argument('--prefix', type=Path, default=Path('build/ffmpeg-media/install'))
parser.add_argument('--output', type=Path, default=Path('build/media-native'))
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
compiler = args.msys_root.resolve() / 'mingw64/bin'
prefix, output = args.prefix.resolve(), args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
for file in (prefix / 'bin').glob('*.dll'):
    shutil.copy2(file, output / file.name)
subprocess.run([str(compiler / 'gcc.exe'), '-O2', '-Wall', '-Wextra', '-Werror', '-shared', '-static-libgcc', '-s',
    '-Wl,--no-insert-timestamp', str(repo / 'tools/win-build/native/dvda-media.c'), '-I' + str(prefix / 'include'),
    '-L' + str(prefix / 'lib'), '-lavformat', '-lavcodec', '-lavutil', '-lswresample', '-lswscale', '-o', str(output / 'dvda-media.dll')],
    env=os.environ | {'PATH': str(compiler) + os.pathsep + os.environ['PATH']}, check=True)
pending = list(output.glob('*.dll')); checked = set()
while pending:
    file = pending.pop()
    if file.name.lower() in checked: continue
    checked.add(file.name.lower())
    assert Pe(file).machine == 0x8664
    for name in Pe(file).imports():
        if (output / name).exists(): continue
        if (compiler / name).exists():
            shutil.copy2(compiler / name, output / name); pending.append(output / name)
        elif not (Path(os.environ['SystemRoot']) / 'System32' / name).exists():
            raise FileNotFoundError('Unresolved native dependency: ' + name)
record = json.loads((prefix / 'build-manifest.json').read_text('utf-8'))
record['bridge_source_sha256'] = hashlib.sha256((repo / 'tools/win-build/native/dvda-media.c').read_bytes()).hexdigest()
record['files'] = {p.name: {'bytes':p.stat().st_size, 'sha256':hashlib.sha256(p.read_bytes()).hexdigest(), 'imports':sorted(Pe(p).imports())} for p in sorted(output.glob('*.dll'))}
(output / 'media-build.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
print(json.dumps({'directory':str(output), 'files':record['files']}, indent=2))
