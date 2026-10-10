"""Exercise the Rust ISO bridge's C ABI without compiling a C consumer.

Python ctypes calls the Rust exports directly; pycdlib independently extracts
the ISO9660/UDF payloads. No retired C author sources or runtime are used.
"""
import argparse
import ctypes
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

import pycdlib

sys.path.insert(0, str(Path(__file__).parent / 'native'))
from pe_dependencies import Pe


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_payloads(image, source):
    expected = {path.relative_to(source).as_posix(): path.read_bytes()
                for path in source.rglob('*') if path.is_file()}
    mounted = pycdlib.PyCdlib()
    mounted.open(str(image))
    try:
        for namespace in ('iso', 'udf'):
            actual = {}
            for directory, _, files in mounted.walk(**{namespace+'_path': '/'}):
                for name in files:
                    path = directory.rstrip('/')+'/'+name
                    output = io.BytesIO()
                    mounted.get_file_from_iso_fp(output, **{namespace+'_path': path})
                    actual[path.lstrip('/').removesuffix(';1')] = output.getvalue()
            require(actual == expected, namespace+' payloads differ from the source tree')
    finally:
        mounted.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', type=Path, default=Path(r'C:\msys64'))
    parser.add_argument('--work-directory', type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    work = (args.work_directory or repo/'build'/('rust-iso-bridge-acceptance-'+uuid.uuid4().hex[:8])).resolve()
    if work.exists() and any(work.iterdir()):
        raise ValueError('Acceptance needs a new or empty work directory: '+str(work))
    work.mkdir(parents=True, exist_ok=True)
    msys_bin = args.msys_root.resolve()/'mingw64/bin'
    env = os.environ | {'PATH': str(msys_bin)+os.pathsep+os.environ['PATH']}
    target = 'x86_64-pc-windows-gnu'
    commands = []

    def run(command, name):
        commands.append([str(part) for part in command])
        result = subprocess.run(command, cwd=repo, env=env, capture_output=True)
        (work/(name+'.log')).write_bytes(result.stdout+result.stderr)
        require(result.returncode == 0, name+' failed; see '+str(work/(name+'.log')))
        return result

    run(['cargo', 'build', '--offline', '--release', '--manifest-path', str(repo/'rust/Cargo.toml'),
         '-p', 'dvda-bridges', '--no-default-features', '--features', 'author-iso', '--lib',
         '--target', target, '--target-dir', str(work/'cargo-target')], 'cargo-build')
    release = work/'cargo-target'/target/'release'
    archive = release/'libdvda_bridges.a'
    library = release/'dvda_bridges.dll'
    source = work/'disc'
    (source/'AUDIO_TS').mkdir(parents=True)
    (source/'VIDEO_TS').mkdir()
    (source/'AUDIO_TS/ATS_01_1.AOB').write_bytes(bytes(range(256))*9)
    dll = ctypes.CDLL(str(library))
    write = dll.dvda_iso_write
    write.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p]
    write.restype = ctypes.c_int
    source_arg = str(source).encode('utf-8')
    output = work/'ffi-call.iso'
    output_arg = str(output).encode('utf-8')
    results = []

    def check(name, arguments, expected):
        actual = write(*arguments)
        require(actual == expected, f'{name}: expected {expected}, got {actual}')
        results.append({'case': name, 'return_code': actual})

    check('explicit-label', [source_arg, output_arg, b'ABI-TEST'], 0)
    read_payloads(output, source)
    check('default-label', [source_arg, output_arg, None], 0)
    require(output.read_bytes()[16*2048+40:16*2048+72] == b'DVD-AUDIO'.ljust(32, b' '),
            'NULL label did not select DVD-AUDIO')
    sentinel = b'existing destination'
    output.write_bytes(sentinel)
    for name, arguments in [
        ('null-source', [None, output_arg, None]),
        ('null-destination', [source_arg, None, None]),
        ('invalid-source-utf8', [b'\xff', output_arg, None]),
        ('invalid-destination-utf8', [source_arg, b'\xff', None]),
        ('invalid-label-utf8', [source_arg, output_arg, b'\xff']),
        ('missing-source', [str(work/'missing').encode('utf-8'), output_arg, None]),
        ('destination-in-source', [source_arg, str(source/'invalid.iso').encode('utf-8'), None]),
    ]:
        check(name, arguments, -1)
        require(output.read_bytes() == sentinel, name+' changed an existing destination')
    require(not list(work.glob('.dvda-iso-*.tmp')), 'ISO writer leaked temporary files')
    unicode_image = work/'音频图像.iso'
    check('unicode-destination', [source_arg, str(unicode_image).encode('utf-8'), '音频'.encode('utf-8')], 0)
    require(unicode_image.is_file(), 'UTF-8 destination was not created')
    read_payloads(unicode_image, source)
    imports = sorted(Pe(library).imports())
    system = Path(os.environ.get('SystemRoot', r'C:\Windows'))/'System32'
    require(all((system/name).is_file() or name.lower().startswith(('api-ms-win-', 'ext-ms-win-'))
                for name in imports), 'ISO-only Rust library imports a non-system DLL: '+repr(imports))
    record = {'schema_version': 1, 'passed': True, 'target': target, 'features': ['author-iso'],
              'consumer': 'python-ctypes', 'project_c_compiled': False,
              'commands': commands, 'library_imports': imports, 'abi_cases': results,
              'reader_checks': ['ISO9660/UDF whole-file payload extraction', 'Unicode destination'],
              'files': {file.relative_to(work).as_posix(): {'sha256': sha(file), 'bytes': file.stat().st_size}
                        for file in [archive, library, output, unicode_image]}}
    report = work/'acceptance.json'
    report.write_text(json.dumps(record, indent=2)+'\n', encoding='utf-8')
    print('Rust ISO bridge acceptance passed: '+str(report), flush=True)


if __name__ == '__main__':
    main()
