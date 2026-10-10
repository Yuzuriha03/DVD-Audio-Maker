"""Check the historical menu DLL's once-per-load session and thread restriction.

Inputs are real menu fixtures from test-menu-rust.py. Production uses the
reusable static adapter; the differential DLL intentionally needs a fresh load.
"""
from pathlib import Path
import argparse
import concurrent.futures
import ctypes
import hashlib
import json
import threading


class Request(ctypes.Structure):
    _fields_ = [('size', ctypes.c_uint), ('xml', ctypes.c_char_p),
                ('input', ctypes.c_char_p), ('output', ctypes.c_char_p),
                ('pixels', ctypes.c_void_p)]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--image-runtime', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    work = args.output.resolve()
    work.mkdir(parents=True, exist_ok=False)
    fixtures = args.fixtures.resolve()
    runtime = args.runtime.resolve()
    image = ctypes.CDLL(str(args.image_runtime.resolve()))
    pixels = ctypes.cast(image.dvda_image_read_rgba, ctypes.c_void_p)
    release = ctypes.windll.kernel32.FreeLibrary
    release.argtypes = [ctypes.c_void_p]
    release.restype = ctypes.c_int
    checks = []

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)
        print('PASS ' + name, flush=True)

    def load(kind):
        library = ctypes.CDLL(str(runtime / ('dvda-menu-' + kind + '.dll')))
        entry = library.dvda_menu_run
        entry.argtypes = [ctypes.POINTER(Request)]
        entry.restype = ctypes.c_int
        return library, entry

    def unload(library):
        check('FreeLibrary', release(library._handle) != 0)

    def tree(path):
        return {f.relative_to(path).as_posix(): f.read_bytes()
                for f in path.rglob('*') if f.is_file()}

    valid_xml = {}
    for candidate in fixtures.glob('*.xml'):
        text = candidate.read_text(encoding='utf-8', errors='replace')
        if text.startswith('<subpictures ') and 'force="yes"' in text:
            valid_xml['spu'] = candidate
        elif text.startswith('<dvdauthor ') and '<amgm>' in text:
            valid_xml['nav'] = candidate
    if set(valid_xml) != {'spu', 'nav'}:
        raise ValueError('Missing real SPU/navigation fixtures')
    source = next(f for f in fixtures.glob('*.mpg')
                  if f.name not in {'legacy.mpg', 'rust.mpg'})
    expected = {'spu': (fixtures / 'rust.mpg').read_bytes(),
                'nav': tree(fixtures / 'rust-nav')}

    for kind in ['spu', 'nav']:
        def request(label, xml=None, size=None):
            target = work / (kind + '-' + label + ('.mpg' if kind == 'spu' else ''))
            if kind == 'nav':
                (target / 'AUDIO_TS').mkdir(parents=True)
                (target / 'VIDEO_TS').mkdir()
            enc = lambda path: str(path).encode('utf-8') if path else None
            value = Request(ctypes.sizeof(Request) if size is None else size,
                            enc(valid_xml[kind] if xml is None else xml),
                            enc(source) if kind == 'spu' else None,
                            enc(target), pixels)
            return value, target

        def contents(target):
            return target.read_bytes() if kind == 'spu' else tree(target)

        def untouched(target):
            return not target.exists() if kind == 'spu' else not tree(target)

        for first in ['null', 'bad-size', 'missing-xml']:
            library, entry = load(kind)
            good, target = request(first + '-good')
            if first == 'null':
                result = entry(None)
            else:
                bad, _ = request(first + '-bad',
                                 xml=work / 'absent.xml' if first == 'missing-xml' else None,
                                 size=0 if first == 'bad-size' else None)
                result = entry(ctypes.byref(bad))
            check(kind + ' ' + first + ' rejects', result != 0)
            check(kind + ' ' + first + ' consumes load',
                  entry(ctypes.byref(good)) != 0 and untouched(target))
            unload(library)

        library, entry = load(kind)
        good, target = request('fresh')
        check(kind + ' fresh load succeeds', entry(ctypes.byref(good)) == 0)
        check(kind + ' fresh load byte parity', contents(target) == expected[kind])
        check(kind + ' second call rejects', entry(ctypes.byref(good)) != 0)
        check(kind + ' second call retains output', contents(target) == expected[kind])
        unload(library)

        library, entry = load(kind)
        good, target = request('race')
        barrier = threading.Barrier(4)

        def invoke(_):
            barrier.wait()
            return entry(ctypes.byref(good))

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(invoke, range(4)))
        check(kind + ' concurrent first call exactly one winner', results.count(0) == 1)
        check(kind + ' concurrent output byte parity', contents(target) == expected[kind])
        check(kind + ' concurrent load remains consumed', entry(ctypes.byref(good)) != 0)
        unload(library)

    record = {'passed': True, 'scope': 'historical-dll-once-per-load',
              'threads': 4, 'checks': checks, 'runtime': str(runtime),
              'runtime_sha256': {kind: sha(runtime / ('dvda-menu-' + kind + '.dll'))
                                 for kind in ['spu', 'nav']},
              'image_sha256': sha(args.image_runtime), 'fixtures': str(fixtures)}
    (work / 'report.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
