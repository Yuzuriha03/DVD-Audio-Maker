"""Compare rebuilt MLP decoding with the original libraries and generated PCM."""
from pathlib import Path
import argparse
import array
import ctypes as c
import hashlib
import json
import math
import os
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent / 'native'))
from pe_dependencies import Pe

LIBRARIES = ['avcodec-63.dll', 'avformat-63.dll', 'avutil-61.dll']


def sha(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


class Stamp(c.Structure):
    _fields_ = [('start', c.c_uint64), ('size', c.c_uint32), ('packet', c.c_void_p), ('valid', c.c_uint32)]


class Config(c.Structure):
    _fields_ = [(name, c.c_uint32) for name in ['size', 'abi', 'rate', 'bits', 'channels', 'restart']] + [
        ('frames', c.c_uint64), ('metadata', c.POINTER(Stamp)), ('metadata_count', c.c_size_t)]


class Result(c.Structure):
    _fields_ = [('status', c.c_int), ('units', c.c_uint32), ('input_frames', c.c_uint64),
                ('encoded_frames', c.c_uint64), ('bytes', c.c_uint64), ('error', c.c_char * 192)]


Read = c.CFUNCTYPE(c.c_int, c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t))
Write = c.CFUNCTYPE(c.c_int, c.c_void_p, c.c_void_p, c.c_size_t)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path, help='Original menu-bin directory')
    parser.add_argument('--candidate', required=True, type=Path, help='Rebuilt DLL directory')
    parser.add_argument('--prefix', required=True, type=Path, help='Rebuilt FFmpeg install prefix')
    parser.add_argument('--msys-root', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--encoder', type=Path, default=Path('build/mlp-encoder/mlp_encoder.dll'),
                        help='Rust encoder acceptance DLL built by dvda-mlp/build-runtime.ps1')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    baseline, candidate, prefix, work = (p.resolve() for p in [args.baseline, args.candidate, args.prefix, args.output])
    work.mkdir(parents=True, exist_ok=False)
    compiler_dir = args.msys_root.resolve() / 'mingw64/bin'
    subprocess.run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                    str(Path(__file__).with_name('build-rust-bridges.ps1')), '-Component', 'media',
                    '-MsysRoot', str(args.msys_root.resolve()), '-FfmpegPrefix', str(prefix),
                    '-Output', str(work / 'rust-probe')], check=True)
    probe = work / 'rust-probe' / 'mlp-decode-probe.exe'
    report = {'status': 'RUNNING', 'cases': [], 'libraries': {}, 'exports_checked': 0}
    for name in LIBRARIES:
        parsed = Pe(candidate / name)
        assert parsed.machine == 0x8664
        report['libraries'][name] = {'sha256': sha(candidate / name), 'bytes': (candidate / name).stat().st_size,
                                    'imports': sorted(parsed.imports())}
    # Check all original consumers, including the new test probe.
    exports = {name: Pe(candidate / name).exports() for name in LIBRARIES}
    for path in list(baseline.glob('*.exe')) + list(baseline.glob('*.dll')) + [probe]:
        if path.name in LIBRARIES:
            continue
        for dependency, names in Pe(path).imports().items():
            if dependency in exports:
                assert names <= exports[dependency], (path.name, dependency, sorted(names - exports[dependency]))
                report['exports_checked'] += len(names)
    system = Path(os.environ['SystemRoot'])

    def run(directory, arguments):
        env = os.environ | {'PATH': os.pathsep.join([str(directory), str(system / 'System32'), str(system)])}
        result = subprocess.run([str(probe)] + [str(x) for x in arguments], env=env, cwd=work,
                                capture_output=True, timeout=60, creationflags=subprocess.CREATE_NO_WINDOW)
        assert result.returncode == 0, result.stderr.decode('utf-8', 'replace')
        loaded = {}
        for line in result.stderr.decode('utf-8', 'replace').splitlines():
            if line.startswith('DLL '):
                _, name, location = line.split(' ', 2)
                loaded[name] = Path(location).resolve()
        assert loaded == {name: (directory / name).resolve() for name in LIBRARIES}, loaded
        return json.loads(result.stdout)

    report['capabilities'] = run(candidate, ['--capabilities'])
    encoder_path = args.encoder.resolve()
    encoder_manifest = json.loads((encoder_path.parent/'encoder-build.json').read_text(encoding='utf-8-sig'))
    assert encoder_manifest['implementation'] == 'rust'
    assert sha(encoder_path) == encoder_manifest['files'][encoder_path.name]['sha256']
    report['encoder'] = {'implementation': 'rust', 'sha256': sha(encoder_path)}
    encoder = c.CDLL(str(encoder_path))
    encode = encoder.mlp_encode_stream_layout
    encode.argtypes = [c.POINTER(Config), c.c_uint32, Read, c.c_void_p, Write, c.c_void_p, c.POINTER(Result)]
    encode.restype = c.c_int
    packet = (c.c_uint8 * 4)(0, 0, 0x40, 0)
    stamp = Stamp(0, 4, c.cast(packet, c.c_void_p), 0)
    assert c.sizeof(Config) == 48 and c.sizeof(Stamp) == 32
    assignments = {1: 0, 2: 1, 3: 7, 4: 3, 5: 9, 6: 12}
    for rate in [44100, 48000, 88200, 96000, 176400, 192000]:
        for bits in [16, 20, 24]:
            for channels in range(1, (2 if rate > 96000 else 6) + 1):
                label = f'{rate}_{bits}_{channels}'
                frames = rate // 5 + 37
                samples = array.array('i')
                noise = 0x12345678
                for i in range(frames):
                    for channel in range(channels):
                        noise = (1664525 * noise + 1013904223) & 0xffffffff
                        value = round((1 << (bits - 4)) * (math.sin(2 * math.pi * (173 + channel * 47) * i / rate)
                                      + 0.31 * math.sin(2 * math.pi * 71 * i / rate))) + ((noise >> 28) - 8)
                        if i < 17 or i >= frames - 13:
                            value = 0
                        samples.append(value << (24 - bits))
                raw = (c.c_int32 * len(samples)).from_buffer(samples)
                position, chunks = 0, []

                @Read
                def read(_, destination, capacity, count):
                    nonlocal position
                    take = min(capacity, frames - position)
                    c.memmove(destination, c.addressof(raw) + position * channels * 4, take * channels * 4)
                    position += take
                    count[0] = take
                    return 0

                @Write
                def write(_, data, length):
                    chunks.append(c.string_at(data, length))
                    return 0

                config = Config(48, 1, rate, bits, channels, 0, frames, c.pointer(stamp), 1)
                result = Result()
                status = encode(c.byref(config), assignments[channels], read, None, write, None, c.byref(result))
                assert status == 0 and result.status == 0 and result.input_frames == frames, (label, status, result.error)
                mlp = work / (label + '.mlp')
                mlp.write_bytes(b''.join(chunks))
                assert mlp.stat().st_size == result.bytes
                old_pcm, new_pcm = work / (label + '.old.s32'), work / (label + '.new.s32')
                old = run(baseline, [mlp, old_pcm])
                new = run(candidate, [mlp, new_pcm])
                assert old == new, (label, old, new)
                assert new['sample_rate'] == rate and new['channels'] == channels and new['bits'] == bits, (label, new)
                data = new_pcm.read_bytes()
                expected = array.array('i', (value << 8 for value in samples)).tobytes()
                assert old_pcm.read_bytes() == data, label + ' old/new decode mismatch'
                assert data[:len(expected)] == expected, label + ' original PCM mismatch'
                assert not any(data[len(expected):]), label + ' nonzero padding'
                assert new['frames'] == result.encoded_frames
                report['cases'].append({'case': label, 'input_frames': frames, 'decoded_frames': new['frames'],
                    'mlp_bytes': result.bytes, 'mlp_sha256': sha(mlp), 'pcm_sha256': sha(new_pcm),
                    'original_pcm_equal': True, 'baseline_decoder_equal': True})
                print('PASS ' + label, flush=True)
    report['status'] = 'PASS'
    (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f'PASS: {len(report["cases"])} combinations; {report["exports_checked"]} imported symbols verified.', flush=True)


if __name__ == '__main__':
    main()
