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

TARGET = 'x86_64-pc-windows-msvc'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def rust_sources(repo):
    result = {}
    for crate in ('dvda-author', 'dvda-core', 'dvda-cli', 'dvda-native', 'dvda-bridges', 'dvda-menu', 'dvda-mlp'):
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
    parser.add_argument('--work-directory', type=Path, default=Path('build/rust-author-production'))
    parser.add_argument('--ffmpeg-runtime', type=Path, default=os.environ.get('DVDA_FFMPEG_PREFIX'))
    parser.add_argument('--magick-work', type=Path, default=os.environ.get('DVDA_MAGICK_WORK'))
    parser.add_argument('--image-dependencies', type=Path, default=os.environ.get('DVDA_IMAGE_DEPS_DIR'),
                        help='Directory containing MSVC-native jpeg/png/webp/zlib .lib dependencies')
    parser.add_argument('--menu-runtime', type=Path, default=Path('build/menu-msvc-vendor'))
    parser.add_argument('--menu-assets', type=Path, help='Directory containing activeheader and black/silent menu assets')
    parser.add_argument('--font', type=Path, help='Prepared DvdaNotoCJK-Regular.ttc collection')
    parser.add_argument('--assets-runtime', type=Path, help='Existing portable runtime supplying data/menu and fonts')
    parser.add_argument('--dependency-runtime', type=Path, help='Retained FFmpeg dependency closure (e.g. libsoxr.dll)')
    parser.add_argument('--target-directory', type=Path, default=Path('rust/target'))
    # Kept for callers transitioning from the bridge-backed C producer. This
    # archive is never linked: all Rust sources are rebuilt by Cargo below.
    parser.add_argument('--rust-bridges', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not args.ffmpeg_runtime or not args.magick_work:
        parser.error('--ffmpeg-runtime and --magick-work (or corresponding DVDA env vars) are required')
    work = args.work_directory.resolve()
    work.mkdir(parents=True, exist_ok=True)
    ffmpeg, magick, menu = [p.resolve() for p in (args.ffmpeg_runtime, args.magick_work, args.menu_runtime)]
    image_dependencies = args.image_dependencies.resolve() if args.image_dependencies else magick/'lib'
    ffmpeg_record = read_json(ffmpeg/'build-manifest.json')
    static_ffmpeg = ffmpeg_record.get('linkage') == 'static'
    if ffmpeg_record.get('profile') != ('static' if static_ffmpeg else 'shared'):
        raise ValueError('Production Rust author requires a complete FFmpeg profile')
    for name, entry in ffmpeg_record['files'].items():
        path = ffmpeg/('lib' if static_ffmpeg else 'bin')/name
        if sha(path) != entry['sha256'] or path.stat().st_size != entry['bytes']:
            raise ValueError('FFmpeg input checksum mismatch: '+name)
    if ffmpeg_record.get('target') != TARGET or ffmpeg_record.get('toolchain') != 'msvc':
        raise ValueError('Production Rust author requires FFmpeg built with Windows x64 MSVC')
    menu_record = read_json(menu/'menu-build.json')
    if menu_record.get('adapter') != 'rust' or menu_record.get('linkage') != 'static-vendor':
        raise ValueError('Production Rust author requires the static-vendor Rust menu adapter')
    if menu_record.get('target') != TARGET or menu_record.get('toolchain') != 'msvc':
        raise ValueError(
            'Production Rust author requires an MSVC-native menu vendor archive; '
            'the retained GNU/MinGW vendor archive cannot be linked into an MSVC binary'
        )
    if menu_record.get('session', {}).get('implementation') != 'rust' or menu_record.get('session', {}).get('boundary') != 'c-setjmp-varargs':
        raise ValueError('Production menu must use the Rust session and C error/varargs boundary')
    if menu_record.get('state_reset', {}).get('implementation') != 'rust':
        raise ValueError('Production menu must use Rust vendor-state reset')
    for relative in ('rust/crates/dvda-menu/src/session.rs',
                     'rust/crates/dvda-menu/src/resources.rs',
                     'rust/crates/dvda-menu/src/direct.rs',
                     'rust/crates/dvda-menu/src/lib.rs'):
        if menu_record.get('rust_inputs', {}).get(relative) != sha(repo/relative):
            raise ValueError('Menu Rust source checksum mismatch: '+relative)
    for name in ('dvda_menu_spu_vendor.lib', 'dvda_menu_nav_vendor.lib'):
        if sha(menu/name) != menu_record['files'][name]['sha256']:
            raise ValueError('Menu archive checksum mismatch: '+name)
    required = [magick/'compile/MagickWand/.libs/MagickWand-7.Q16HDRI.lib',
                magick/'compile/MagickCore/.libs/MagickCore-7.Q16HDRI.lib',
                magick/'freetype-minimal.lib']
    for path in required:
        if not path.is_file():
            raise FileNotFoundError('MSVC-native image .lib dependency is missing: '+str(path))
    for name in ('jpeg.lib', 'png16.lib', 'webpdecoder.lib', 'webpmux.lib', 'z.lib'):
        path = image_dependencies/name
        if not path.is_file():
            raise FileNotFoundError('MSVC-native image codec .lib dependency is missing: '+str(path))
        required.append(path)
    env = msvc_environment()
    env.update({
        'PATH': str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH'],
        'DVDA_FFMPEG_PREFIX': str(ffmpeg), 'DVDA_MAGICK_WORK': str(magick),
        'DVDA_MENU_NATIVE_DIR': str(menu), 'DVDA_IMAGE_DEPS_DIR': str(image_dependencies),
    })
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -C target-feature=+crt-static'
    target = args.target_directory.resolve()
    command = ['cargo', 'build', '--offline', '--release', '--manifest-path', str(repo/'rust/Cargo.toml'),
               '--target', TARGET, '--target-dir', str(target), '-p', 'dvda-cli', '--bin', 'dvda-author-dev',
               '--no-default-features', '--features', 'direct-bridges,rust-mlp']
    inputs = rust_sources(repo)
    with (work/'rust-author-build.log').open('wb') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError('Rust author build failed; see '+str(work/'rust-author-build.log'))
    if rust_sources(repo) != inputs:
        raise RuntimeError('Rust sources changed during this build; rerun to authenticate a stable binary')
    executable = work/'dvda-author-dev.exe'
    shutil.copy2(target/TARGET/'release/dvda-author-dev.exe', executable)
    if Pe(executable).machine != 0x8664:
        raise ValueError('Rust author is not Windows x64')
    system = Path(os.environ.get('SystemRoot', r'C:\Windows'))/'System32'
    roots = [ffmpeg/'bin']
    if args.dependency_runtime:
        roots.append(args.dependency_runtime.resolve())
    roots += [repo/'build/msvc-native/media-runtime', *vc_runtime_roots(env)]
    pending, seen, runtime_sources = [executable], set(), {}
    while pending:
        path = pending.pop()
        for name in Pe(path).imports():
            if name.startswith(('api-ms-win-', 'ext-ms-win-')) or ((system/name).is_file() and not is_vc_runtime(name)):
                continue
            if name.startswith('dvda-') or name == 'mlp_encoder.dll':
                raise ValueError('Rust author imports a migrated project adapter: '+name)
            if name in seen:
                continue
            source = next((root/name for root in roots if (root/name).is_file()), None)
            if source is None:
                raise FileNotFoundError('Unresolved Rust author dependency: '+name)
            destination = work/name
            # Repeated builds can share unchanged DLLs with an active runtime
            # acceptance process. Windows forbids deleting a loaded library.
            if not destination.is_file() or sha(destination) != sha(source):
                shutil.copy2(source, destination)
            runtime_sources[name] = {'path': str(source), 'sha256': sha(source)}
            seen.add(name)
            pending.append(destination)
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
    for name in ('menu-build.json', 'dvda_menu_spu_vendor.lib', 'dvda_menu_nav_vendor.lib'):
        inputs['menu-runtime/'+name] = sha(menu/name)
    shutil.copy2(menu/'menu-build.json', work/'menu-build.json')
    if (menu/'NOTICE.txt').is_file():
        shutil.copy2(menu/'NOTICE.txt', work/'menu-NOTICE.txt')
    dependencies = {}
    for root in (ffmpeg/'include', ffmpeg/'lib', ffmpeg/'bin'):
        for path in sorted(root.rglob('*')):
            if path.is_file():
                dependencies[str(path)] = sha(path)
    for path in [ffmpeg/'build-manifest.json', magick/'image-build.json', *required,
                 menu/'menu-build.json', menu/'dvda_menu_spu_vendor.lib', menu/'dvda_menu_nav_vendor.lib']:
        dependencies[str(path)] = sha(path)
    for pattern in ('compile/config/*.h','ImageMagick-7.0.8-47/MagickCore/*.h'):
        for path in sorted(magick.glob(pattern)):
            dependencies[str(path)] = sha(path)
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
    (work/'policy.xml').write_text('<policymap>\n  <policy domain="delegate" rights="none" pattern="*"/>\n</policymap>\n',encoding='utf-8')
    (work/'colors.xml').write_text('<colormap/>\n',encoding='utf-8')
    typemap='<typemap>'
    for face, language in enumerate(('SC','JP','KR')):
        for prefix in ('DVDA-',''):
            typemap+=f'<type name="{prefix}Noto-Sans-CJK-{language}" family="Noto Sans CJK {language}" format="truetype" style="normal" stretch="normal" weight="400" face="{face}" glyphs="fonts/DvdaNotoCJK-Regular.ttc"/>'
    (work/'type.xml').write_text(typemap+'</typemap>',encoding='utf-8')
    runtime_files={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'imports':sorted(Pe(p).imports())} for p in sorted(work.glob('*.dll'))}
    assets={p.relative_to(work).as_posix():{'sha256':sha(p),'bytes':p.stat().st_size} for p in sorted([*work.glob('*.xml'),*work.glob('fonts/*'),*work.glob('data/menu/*')]) if p.is_file()}
    record={'schema_version':1,'implementation':'rust','target':TARGET,'toolchain':'msvc',
        'project_c_author_compiled':False,'menu_linkage':'direct-static-vendor',
        'ffmpeg_linkage':'static-source-built' if static_ffmpeg else 'shared-source-built-shared-profile',
        'ffmpeg_profile':'build-msvc-ffmpeg.py:'+ffmpeg_record['profile'],
        'ffmpeg_manifest':{'path':str(ffmpeg/'build-manifest.json'),'sha256':sha(ffmpeg/'build-manifest.json')},
        'author_library':{'implementation':'rust','available':True,'entry':'dvda_core::author_runtime::execute','serialization':'request-owned'},
        'iso_writer':{'implementation':'project-owned-rust','linkage':'embedded-rust-author','source_inputs':{k:v for k,v in inputs.items() if k.startswith('rust/crates/dvda-author/')}},
        'compiler':subprocess.check_output(['rustc','--version'],env=env).decode().strip(),
        'cargo_arguments':command[1:],'source_inputs':inputs,'third_party_inputs':dependencies,
        'runtime_dependency_inputs':runtime_sources,'runtime_asset_inputs':asset_sources,'runtime_assets':assets,
        'files':{executable.name:{'sha256':sha(executable),'bytes':executable.stat().st_size,'imports':sorted(Pe(executable).imports())}},'runtime_files':runtime_files}
    (work/'author-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'author':str(executable),'implementation':'rust','runtime_dlls':sorted(runtime_files),'menu_pages':'generated by Rust author'}),flush=True)


if __name__=='__main__':
    main()
