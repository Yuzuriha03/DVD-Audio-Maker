"""Self-contained frozen-C/Rust menu boundary and byte differential regression."""
from pathlib import Path
import argparse, ctypes, hashlib, json, subprocess

class Request(ctypes.Structure):
    _fields_ = [('size', ctypes.c_uint), ('xml', ctypes.c_char_p), ('input', ctypes.c_char_p), ('output', ctypes.c_char_p), ('pixels', ctypes.c_void_p)]

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime', type=Path, required=True)
    p.add_argument('--oracle', type=Path, required=True)
    p.add_argument('--image-runtime', type=Path, required=True)
    p.add_argument('--assets', type=Path, required=True)
    p.add_argument('--ffmpeg', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--format', choices=['pal', 'ntsc'], default='pal')
    p.add_argument('--pgcs', type=int, choices=[1, 2], default=1)
    a = p.parse_args(); out = a.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    image = ctypes.CDLL(str(a.image_runtime.resolve()))
    pixels = ctypes.cast(image.dvda_image_read_rgba, ctypes.c_void_p)
    release = ctypes.windll.kernel32.FreeLibrary
    release.argtypes = [ctypes.c_void_p]; release.restype = ctypes.c_int
    def run(runtime, kind, xml, target, source=None):
        lib = ctypes.CDLL(str((runtime / ('dvda-menu-' + kind + '.dll')).resolve()))
        entry = lib.dvda_menu_run; entry.argtypes = [ctypes.POINTER(Request)]; entry.restype = ctypes.c_int
        enc = lambda path: str(path).encode('utf-8') if path else None
        req = Request(ctypes.sizeof(Request), enc(xml), enc(source), enc(target), pixels)
        try: return entry(ctypes.byref(req))
        finally: release(lib._handle)
    source = out / '背景.mpg'
    picture = out / '图像.png'
    subprocess.run([str(a.ffmpeg), '-v', 'error', '-y', '-i', str(a.assets / 'black_PAL_720x576.png'), '-vf', 'scale=720:' + ('576' if a.format == 'pal' else '480'), '-frames:v', '1', str(picture)], check=True)
    subprocess.run([str(a.ffmpeg), '-v', 'error', '-y', '-loop', '1', '-i', str(picture), '-i', str(a.assets / 'silence.wav'), '-t', '0.6', '-target', a.format + '-dvd', '-acodec', 'mp2', '-b:a', '192k', str(source)], check=True)
    xml = out / '菜单.xml'
    image_path = str(picture).replace('&', '&amp;')
    xml.write_text('<subpictures format="' + a.format.upper() + '"><stream><spu force="yes" start="00:00:00.00" image="' + image_path + '" highlight="' + image_path + '" select="' + image_path + '"><button name="button01" x0="20" y0="56" x1="700" y1="100"/></spu></stream></subpictures>', encoding='utf-8')
    checks = []
    def check(name, ok):
        if not ok: raise AssertionError(name)
        checks.append(name); print('PASS ' + name, flush=True)
    legacy = out / 'legacy.mpg'; current = out / 'rust.mpg'
    check('legacy SPU', run(a.oracle, 'spu', xml, legacy, source) == 0)
    check('Rust UTF8 SPU', run(a.runtime, 'spu', xml, current, source) == 0)
    check('SPU byte parity', legacy.read_bytes() == current.read_bytes())
    navxml = out / '导航.xml'
    pgcs = ''.join('<pgc><pre>g0=' + str(i) + ';</pre><button name="button01">jump menu ' + str(i % a.pgcs + 1) + ';</button><vob file="' + str(current) + '"/><post>jump menu ' + str(i % a.pgcs + 1) + ';</post></pgc>' for i in range(1, a.pgcs + 1))
    navxml.write_text('<dvdauthor jumppad="1"><amgm><menus><video format="' + a.format + '"/><audio format="mp2" lang="en"/>' + pgcs + '</menus></amgm></dvdauthor>', encoding='utf-8')
    trees = []
    for name, runtime in [('legacy-nav', a.oracle), ('rust-nav', a.runtime)]:
        target = out / name; target.mkdir(); (target / 'AUDIO_TS').mkdir(); (target / 'VIDEO_TS').mkdir()
        check(name, run(runtime, 'nav', navxml, target) == 0)
        trees.append({str(f.relative_to(target)): f.read_bytes() for f in target.rglob('*') if f.is_file()})
    check('navigation byte parity', bool(trees[0]) and trees[0] == trees[1])
    bad = out / '错误.xml'; failed = out / '失败.mpg'
    cases = ['', '<a>', '<subpictures><stream><unknown/></stream></subpictures>', '<!DOCTYPE subpictures><subpictures/>', '<subpictures><stream><spu force="invalid"/></stream></subpictures>', '<subpictures><stream><spu image="missing.png"/></stream></subpictures>', '<subpictures>\x00</subpictures>', '<subpictures>&#1;</subpictures>', '<!--a--><?xml version="1.0"?><subpictures/>', '<subpictures a="x" a="y"/>']
    check('missing XML', run(a.runtime, 'spu', out / 'absent.xml', failed, source) != 0)
    check('missing MPEG', run(a.runtime, 'spu', xml, failed, out / 'absent.mpg') != 0)
    bad.write_bytes(b'<subpictures>\xff</subpictures>')
    check('invalid UTF8', run(a.runtime, 'spu', bad, failed, source) != 0)
    check('unwritable output', run(a.runtime, 'spu', xml, out / 'absent' / 'output.mpg', source) != 0)
    invalid_nav = out / 'invalid-nav.xml'
    invalid_nav.write_text(navxml.read_text(encoding='utf-8').replace('g0=1;', 'not_a_vm_command;'), encoding='utf-8')
    invalid_target = out / 'invalid-nav'; invalid_target.mkdir()
    (invalid_target / 'AUDIO_TS').mkdir(); (invalid_target / 'VIDEO_TS').mkdir()
    check('vendor invalid VM command', run(a.runtime, 'nav', invalid_nav, invalid_target) != 0)
    handles = ctypes.c_ulong()
    kernel = ctypes.windll.kernel32
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.GetProcessHandleCount.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
    kernel.GetProcessHandleCount.restype = ctypes.c_int
    check('initial handle query', bool(kernel.GetProcessHandleCount(kernel.GetCurrentProcess(), ctypes.byref(handles)))); before = handles.value
    for cycle in range(12):
        for index, text in enumerate(cases):
            bad.write_text(text, encoding='utf-8')
            check(f'failure {cycle}/{index}', run(a.runtime, 'spu', bad, failed, source) != 0)
            if failed.exists(): failed.unlink()
        check(f'recovery {cycle}', run(a.runtime, 'spu', xml, current, source) == 0 and current.read_bytes() == legacy.read_bytes())
    check('final handle query', bool(kernel.GetProcessHandleCount(kernel.GetCurrentProcess(), ctypes.byref(handles))))
    check('no leaked Windows handles', handles.value <= before + 2)
    provenance = {label: {'path': str(runtime.resolve()), 'dll_sha256': {kind: hashlib.sha256((runtime / ('dvda-menu-' + kind + '.dll')).read_bytes()).hexdigest() for kind in ['spu', 'nav']}} for label, runtime in [('runtime', a.runtime), ('oracle', a.oracle)]}
    (out / 'report.json').write_text(json.dumps({'status': 'PASS', 'format': a.format, 'pgcs': a.pgcs, 'provenance': provenance, 'checks': checks}, indent=2), encoding='utf-8')

if __name__ == '__main__': main()
