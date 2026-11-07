"""Build and stage the complete Rust author with retained third-party libraries.

No project C author translation unit or legacy author archive is compiled/linked.
The configured --source directory is used only to locate optional menu assets.
MSVC-native vendor archives are required; GNU/MinGW archives are rejected.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).parent / 'native'))
from pe_dependencies import Pe
from msvc_env import is_vc_runtime, msvc_environment, vc_runtime_roots

TARGETS = {'x64': 'x86_64-pc-windows-msvc', 'arm64': 'aarch64-pc-windows-msvc'}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def rust_sources(repo):
    result = {}
    for crate in ('dvda-author', 'dvda-core', 'dvda-cli', 'dvda-native', 'dvda-bridges', 'dvda-menu', 'dvda-mlp', 'dvda-image'):
        root = repo/'rust/crates'/crate
        for path in sorted(root.rglob('*')):
            if path.is_file() and (path.suffix in ('.rs', '.toml') or path.name.endswith('-probe.c')) and not any(part in ('tests','examples') for part in path.relative_to(root).parts):
                result[path.relative_to(repo).as_posix()] = sha(path)
    for path in [repo/'rust/Cargo.toml', repo/'rust/Cargo.lock', repo/'rust/.cargo/config.toml',
                 repo/'.cargo/config.toml', repo/'rust/rust-toolchain.toml', Path(__file__),
                 Path(__file__).with_name('build-image-author.py'), Path(__file__).with_name('msvc_env.py')]:
        result[path.relative_to(repo).as_posix()] = sha(path)
    return result


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, help='Legacy configured source, used only for menu assets')
    parser.add_argument('--msys-root', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--arch', choices=tuple(TARGETS), default='x64',
                        help='Target architecture for the author runtime (default x64)')
    parser.add_argument('--work-directory', type=Path,
                        help='Defaults to build/rust-author-production (x64) or rust-author-production-arm64')
    parser.add_argument('--temp-directory', type=Path, default=repo/'build/tmp',
                        help='Linker temporary directory (defaults to build/tmp inside the workspace)')
    parser.add_argument('--ffmpeg-runtime', type=Path, default=os.environ.get('DVDA_FFMPEG_PREFIX'))
    parser.add_argument('--magick-work', type=Path, help='Retired; supplying this option is an error')
    parser.add_argument('--image-dependencies', type=Path, help='Retired; supplying this option is an error')
    parser.add_argument('--menu-assets', type=Path, help='Directory containing activeheader and black/silent menu assets')
    parser.add_argument('--font', type=Path, help='Prepared DvdaNotoCJK-Regular.ttc collection')
    parser.add_argument('--assets-runtime', type=Path, help='Existing portable runtime supplying data/menu and fonts')
    parser.add_argument('--dependency-runtime', type=Path, help='Retained FFmpeg dependency closure (e.g. libsoxr.dll)')
    parser.add_argument('--target-directory', type=Path, default=Path('rust/target'))
    # Kept for callers transitioning from the bridge-backed C producer. This
    # archive is never linked: all Rust sources are rebuilt by Cargo below.
    parser.add_argument('--rust-bridges', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.magick_work or args.image_dependencies:
        parser.error('--magick-work and --image-dependencies are retired; Rust images need neither')
    if not args.ffmpeg_runtime:
        parser.error('--ffmpeg-runtime (or DVDA_FFMPEG_PREFIX) is required')
    target = TARGETS[args.arch]
    work = (args.work_directory or Path('build/rust-author-production' if args.arch == 'x64' else 'build/rust-author-production-arm64')).resolve()
    work.mkdir(parents=True, exist_ok=True)
    ffmpeg = args.ffmpeg_runtime.resolve()
    ffmpeg_record = read_json(ffmpeg/'build-manifest.json')
    static_ffmpeg = ffmpeg_record.get('linkage') == 'static'
    if ffmpeg_record.get('profile') != ('static' if static_ffmpeg else 'shared'):
        raise ValueError('Production Rust author requires a complete FFmpeg profile')
    for name, entry in ffmpeg_record['files'].items():
        path = ffmpeg/('lib' if static_ffmpeg else 'bin')/name
        if sha(path) != entry['sha256'] or path.stat().st_size != entry['bytes']:
            raise ValueError('FFmpeg input checksum mismatch: '+name)
    if ffmpeg_record.get('target') != target or ffmpeg_record.get('toolchain') != 'msvc':
        raise ValueError(f'Production Rust author requires FFmpeg built with Windows MSVC for {target}')
    temporary = args.temp_directory.resolve()
    temporary.mkdir(parents=True, exist_ok=True)
    env = msvc_environment(arch=args.arch)
    env.update({
        'PATH': str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH'],
        'DVDA_FFMPEG_PREFIX': str(ffmpeg),
        # Always set the child process temporary directory. setdefault() would
        # silently retain an inherited system TEMP and can hit LNK1104.
        'TMP': str(temporary),
        'TEMP': str(temporary),
    })
    for name in ('DVDA_MAGICK_WORK', 'DVDA_IMAGE_DEPS_DIR'):
        env.pop(name, None)
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -C target-feature=+crt-static'
    target_dir = args.target_directory.resolve()
    command = ['cargo', 'build', '--offline', '--release', '--manifest-path', str(repo/'rust/Cargo.toml'),
               '--target', target, '--target-dir', str(target_dir), '-p', 'dvda-core',
               '--features', 'rust-mlp']
    inputs = rust_sources(repo)
    with (work/'rust-author-build.log').open('wb') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError('Rust author build failed; see '+str(work/'rust-author-build.log'))
    if rust_sources(repo) != inputs:
        raise RuntimeError('Rust sources changed during this build; rerun to authenticate a stable binary')
    # The Rust author is linked into the GUI (dvda-core author_runtime); no separate
    # executable is produced. Only the shared FFmpeg DLLs below are runtime inputs.
    runtime_sources, seen = {}, set()
    # Keep the complete authenticated FFmpeg shared profile beside the author
    # record. Some libraries are delay-loaded only by the GUI, so PE traversal
    # of the standalone author cannot discover them.
    for name in (() if static_ffmpeg else ('avcodec-63.dll', 'avformat-63.dll', 'avutil-61.dll',
                 'swresample-7.dll', 'swscale-10.dll')):
        source = ffmpeg/'bin'/name
        if not source.is_file():
            raise FileNotFoundError('Authenticated FFmpeg DLL is missing: '+str(source))
        destination = work/name
        if not destination.is_file() or sha(destination) != sha(source):
            shutil.copy2(source, destination)
        seen.add(name)
        runtime_sources[name] = {'path': str(source), 'sha256': sha(source)}
    # Reconcile only obsolete runtime DLLs in this dedicated output directory.
    for path in work.glob('*.dll'):
        if path.name.lower() not in seen:
            path.unlink()
    dependencies = {}
    for root in (ffmpeg/'include', ffmpeg/'lib', ffmpeg/'bin'):
        for path in sorted(root.rglob('*')):
            if path.is_file():
                dependencies[str(path)] = sha(path)
    dependencies[str(ffmpeg/'build-manifest.json')] = sha(ffmpeg/'build-manifest.json')
    menu_assets = args.menu_assets
    runtime = args.assets_runtime.resolve() if args.assets_runtime else None
    if not menu_assets:
        candidates = ([runtime/'data/menu', runtime/'menu'] if runtime else [])
        if args.source:
            candidates += [args.source.resolve()/'menu', args.source.resolve().parent/'menu']
        menu_assets = next((p for p in candidates if (p/'activeheader').is_file()), None)
    if not menu_assets:
        raise FileNotFoundError('Supply --menu-assets or --assets-runtime with prepared menu assets')
    menu_assets = menu_assets.resolve()
    asset_sources = {}
    for name in ('activeheader','black_NTSC_720x480.jpg','black_NTSC_720x480.png','black_PAL_720x576.jpg','black_PAL_720x576.png','silence.wav'):
        source = menu_assets/name
        destination = work/'data/menu'/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source,destination)
        asset_sources[destination.relative_to(work).as_posix()] = {'path':str(source),'sha256':sha(source)}
    font = args.font
    if not font:
        roots = ([runtime/'fonts'] if runtime else [])+[menu_assets.parent/'fonts', menu_assets/'fonts']
        font = next((p/'DvdaNotoCJK-Regular.ttc' for p in roots if (p/'DvdaNotoCJK-Regular.ttc').is_file()), None)
    if not font:
        raise FileNotFoundError('Supply --font with a prepared DvdaNotoCJK-Regular.ttc collection')
    destination = work/'fonts/DvdaNotoCJK-Regular.ttc'
    destination.parent.mkdir(exist_ok=True)
    shutil.copy2(font,destination)
    asset_sources['fonts/DvdaNotoCJK-Regular.ttc'] = {'path':str(font.resolve()),'sha256':sha(font)}
    assets={name:{'sha256':sha(work/name),'bytes':(work/name).stat().st_size} for name in sorted(asset_sources)}
    runtime_files={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'imports':sorted(Pe(p).imports())} for p in sorted(work.glob('*.dll'))}
    record={'schema_version':1,'implementation':'rust','target':target,'toolchain':'msvc',
        'project_c_author_compiled':False,'menu_linkage':'in-process-rust',
        'ffmpeg_linkage':'static-source-built' if static_ffmpeg else 'shared-source-built-shared-profile',
        'ffmpeg_profile':'build-msvc-ffmpeg.py:'+ffmpeg_record['profile'],
        'ffmpeg_manifest':{'path':str(ffmpeg/'build-manifest.json'),'sha256':sha(ffmpeg/'build-manifest.json')},
        'author_library':{'implementation':'rust','available':True,'entry':'dvda_core::author_runtime::execute','serialization':'request-owned'},
        'iso_writer':{'implementation':'project-owned-rust','linkage':'embedded-rust-author','source_inputs':{k:v for k,v in inputs.items() if k.startswith('rust/crates/dvda-author/')}},
        'compiler':subprocess.check_output(['rustc','--version'],env=env).decode().strip(),
        'cargo_arguments':command[1:],'source_inputs':inputs,'third_party_inputs':dependencies,
        'runtime_dependency_inputs':runtime_sources,'runtime_asset_inputs':asset_sources,'runtime_assets':assets,
        'files':{},'runtime_files':runtime_files}
    (work/'author-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'author':'linked into the GUI (dvda-core author_runtime)','implementation':'rust','runtime_dlls':sorted(runtime_files),'menu_pages':'generated by Rust author'}),flush=True)


if __name__=='__main__':
    main()
