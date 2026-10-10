"""Accept a staged complete Rust author using its shipped runtime only.

This creates an MLP disc and a two-group/two-title integer PCM disc, writes
ISO/UDF images, then reads every output file through both independent readers.
It compiles no C oracle and records executable/dependency/asset provenance.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
import wave

sys.path.insert(0, str(Path(__file__).parent / 'native'))
from pe_dependencies import Pe


def require(value, message):
    if not value:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pcm_fixture(path, channels, bits, frames):
    payload = bytearray()
    scale = 1 << (bits - 16)
    for frame in range(frames):
        for channel in range(channels):
            sample = (((frame * 97 + channel * 1237) % 60001) - 30000) * scale
            payload.extend(sample.to_bytes(bits // 8, 'little', signed=True))
    with wave.open(str(path), 'wb') as output:
        output.setnchannels(channels)
        output.setsampwidth(bits // 8)
        output.setframerate(48000)
        output.writeframes(payload)


def validate_runtime(runtime, record):
    require(record.get('implementation') == 'rust', 'Author must identify the Rust implementation')
    require(record.get('project_c_author_compiled') is False, 'Production build compiled project C author')
    require(record.get('iso_writer', {}).get('linkage') == 'embedded-rust-author',
            'ISO writer is not embedded in the complete Rust author')
    require(record.get('author_library', {}).get('implementation') == 'rust', 'Missing Rust library entry')
    menu_path = runtime / 'menu-build.json'
    require(sha(menu_path) == record['source_inputs'].get('menu-runtime/menu-build.json'),
            'Rust author menu manifest checksum mismatch')
    menu = json.loads(menu_path.read_text(encoding='utf-8-sig'))
    require(menu.get('session', {}).get('implementation') == 'rust'
            and menu.get('session', {}).get('boundary') == 'c-setjmp-varargs',
            'Production menu must use the Rust session')
    require(menu.get('state_reset', {}).get('implementation') == 'rust',
            'Production menu must use the generated Rust reset')
    for relative in ('rust/crates/dvda-menu/src/session.rs',
                     'rust/crates/dvda-menu/src/resources.rs',
                     'rust/crates/dvda-menu/src/direct.rs',
                     'rust/crates/dvda-menu/src/lib.rs'):
        require(menu['rust_inputs'].get(relative) == record['source_inputs'].get(relative),
                'Rust author/menu source checksum mismatch: '+relative)
    files = record['files'] | record['runtime_files'] | record['runtime_assets']
    for name, information in files.items():
        path = runtime / name
        require(path.is_file() and sha(path) == information['sha256'], 'Staged file hash mismatch: '+name)
    declared = set(record['runtime_files'])
    require({path.name for path in runtime.glob('*.dll')} == declared, 'Staged DLL inventory differs')
    system = Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'System32'
    for name in list(record['files']) + list(record['runtime_files']):
        binary = Pe(runtime / name)
        require(binary.machine == 0x8664, 'Staged binary must be Windows x64: '+name)
        for imported in binary.imports():
            require(not imported.startswith('dvda-') and imported != 'mlp_encoder.dll',
                    'Production imports migrated project adapter: '+imported)
            require(imported in declared or imported.startswith(('api-ms-win-', 'ext-ms-win-'))
                    or (system / imported).is_file(), 'Missing shipped dependency: '+imported)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--mlp', type=Path, default=Path('build/formats-real-parity/96000_24_6.aligned.mlp'))
    parser.add_argument('--menu', action='store_true', help='Also author CJK menu text and a still picture from shipped assets')
    parser.add_argument('--work-directory', type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    work = (args.work_directory or repo / 'build' / ('rust-author-production-gate-'+uuid.uuid4().hex[:8])).resolve()
    require(not work.exists() or not any(work.iterdir()), 'Acceptance needs a new or empty directory: '+str(work))
    work.mkdir(parents=True, exist_ok=True)
    runtime = args.runtime.resolve()
    record_path = runtime / 'author-build.json'
    record = json.loads(record_path.read_text(encoding='utf-8-sig'))
    validate_runtime(runtime, record)
    executable = runtime / 'dvda-author-dev.exe'
    # Restrict PATH to the staged runtime and Windows directories: an omitted
    # third-party dependency cannot be supplied by the development environment.
    windows = Path(os.environ.get('SystemRoot', r'C:\Windows'))
    env = os.environ | {'PATH': os.pathsep.join(str(p) for p in (runtime, windows / 'System32', windows))}
    for variable in ('DVDA_RUNTIME_DIRECTORY', 'DVDA_AUTHOR_BIN', 'DVDA_IMAGE_NATIVE_DIR',
                     'MAGICK_CONFIGURE_PATH', 'MAGICK_HOME', 'DVDA_MENU_DATA', 'DVDA_FONT_DIR'):
        env.pop(variable, None)
    version_command = [str(executable), '--version']
    result = subprocess.run(version_command, cwd=work, env=env, capture_output=True)
    version = (result.stdout + result.stderr).decode('utf-8', errors='replace')
    (work / 'version.log').write_text(version, encoding='utf-8')
    require(result.returncode == 0 and 'dvda-author Rust' in version, 'Rust author --version failed')
    mlp = args.mlp.resolve()
    require(mlp.is_file(), 'MLP fixture missing: '+str(mlp))
    pcm16, pcm24 = work / 'stereo-16.wav', work / 'surround-24.wav'
    pcm_fixture(pcm16, 2, 16, 6001)
    pcm_fixture(pcm24, 6, 24, 10003)
    # Share the independent ISO9660/UDF reader with the complete Rust acceptance.
    spec = importlib.util.spec_from_file_location('iso_reader', Path(__file__).with_name('test-rust-author.py'))
    reader = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reader)
    cases = [('mlp-surround', ['-g', str(mlp)]),
             ('pcm-multigroup', ['-g', str(pcm16), '-z', str(pcm16), '-g', str(pcm24)])]
    if args.menu:
        cases.append(('menu-still', ['-g', str(pcm16), '--topmenu', '--fontname', 'DVDA-Noto-Sans-CJK-SC',
                                     '--fontname-jp', 'DVDA-Noto-Sans-CJK-JP', '--screentext', '测试-日本語=发布=音乐',
                                     '--stillpics', str(runtime / 'data/menu/black_PAL_720x576.png')]))
    reports = []
    for name, inputs in cases:
        output, image = work / (name + '-disc'), work / (name + '.iso')
        command = [str(executable), *inputs, '-o', str(output), '-D', str(work / (name + '-temp')),
                   '-W', '-P0', '--iso='+str(image), '--iso-volume', 'RUST-PRODUCTION']
        result = subprocess.run(command, cwd=work, env=env, capture_output=True)
        log = result.stdout + result.stderr
        (work / (name + '.log')).write_bytes(log)
        require(result.returncode == 0, name+' author failed; see '+str(work / (name + '.log')))
        require(b'Rust DVD-Audio author complete' in log, name+' did not complete through Rust author')
        require(image.is_file() and image.stat().st_size > 0, name+' image missing')
        require((output / 'AUDIO_TS/AUDIO_TS.IFO').is_file(), name+' AMG missing')
        require((output / 'AUDIO_TS/AUDIO_PP.IFO').is_file(), name+' SAMG missing')
        if name == 'pcm-multigroup':
            require((output / 'AUDIO_TS/ATS_02_0.IFO').is_file(), 'Second PCM group missing')
        if name == 'menu-still':
            for required in ('AUDIO_TS.VOB', 'AUDIO_SV.VOB', 'AUDIO_SV.IFO'):
                require((output / 'AUDIO_TS' / required).is_file(), 'Menu/still output missing: '+required)
        _, file_count = reader.filesystem(image, output)
        reports.append({'case': name, 'command': command,
                        'iso': {'sha256': sha(image), 'bytes': image.stat().st_size},
                        'reader_checks': [{'filesystem': namespace, 'files': file_count, 'payloads_match': True}
                                          for namespace in ('iso', 'udf')]})
    report = {'schema_version': 1, 'passed': True, 'implementation': 'rust',
              'runtime': str(runtime), 'author_manifest_sha256': sha(record_path),
              'executable_sha256': sha(executable), 'version': version.strip(),
              'runtime_dlls': sorted(record['runtime_files']),
              'input_hashes': {str(path): sha(path) for path in (mlp, pcm16, pcm24)}, 'cases': reports}
    report_path = work / 'production-acceptance.json'
    report_path.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print('Complete Rust author runtime acceptance passed: '+str(report_path), flush=True)


if __name__ == '__main__':
    main()
