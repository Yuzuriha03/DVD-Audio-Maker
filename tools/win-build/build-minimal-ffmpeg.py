"""Build native Windows x64 FFmpeg DLLs for dvda-author's MLP interface.

Optional maintenance tool; normal GUI builds do not need MSYS2 or Python.
Source and NASM archives are pinned. Nothing is installed in the MSYS2 tree.
"""
from pathlib import Path
import argparse
import ctypes
import hashlib
import json
import os
import shlex
import subprocess
import tarfile
import urllib.request
import zipfile

VERSION = '9.0.2'
SOURCE_SHA256 = '8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'
SIGNER = 'FCF986EA15E6E293A5644F10B4322F04D67658D8'
NASM_VERSION = '2.16.03'
NASM_SHA256 = '3ee4782247bcb874378d02f7eab4e294a84d3d15f3f6ee2de2f47a46aa7226e6'
DLL_NAMES = ('avcodec-63.dll', 'avformat-63.dll', 'avutil-61.dll')


def sha(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def posix(path):
    # Keep Windows 8.3 spelling; resolve() expands it back to a spaced path.
    path = Path(path).absolute().as_posix()
    if len(path) < 3 or path[1:3] != ':/':
        raise ValueError('An absolute Windows drive path is required: ' + path)
    return '/' + path[0].lower() + path[2:]


def short_path(path):
    buffer = ctypes.create_unicode_buffer(32768)
    call = ctypes.windll.kernel32.GetShortPathNameW
    call.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint]
    call.restype = ctypes.c_uint
    size = call(str(Path(path).resolve()), buffer, len(buffer))
    if not size or size >= len(buffer):
        raise OSError('Cannot get the short build path: ' + str(path))
    if any(c.isspace() for c in buffer.value):
        raise ValueError('Use --work-directory on a path without spaces (8.3 names unavailable).')
    return posix(buffer.value)


def download(url, target, expected=None):
    if not target.exists():
        print('Downloading ' + url, flush=True)
        request = urllib.request.Request(url, headers={'User-Agent': 'DVD-Audio-Maker-native-build'})
        with urllib.request.urlopen(request, timeout=90) as response:
            data = response.read()
        if expected and hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('Downloaded archive hash mismatch: ' + url)
        target.write_bytes(data)
    if expected and sha(target) != expected:
        raise ValueError('Cached archive hash mismatch: ' + str(target))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', type=Path, default=Path(os.environ.get('MSYS2_ROOT', 'C:/msys64')))
    parser.add_argument('--work-directory', type=Path, default=Path(__file__).resolve().parents[2] / 'build/ffmpeg-minimal')
    parser.add_argument('--jobs', type=int, default=min(8, os.cpu_count() or 1))
    parser.add_argument('--profile', choices=['mlp', 'media'], default='mlp')
    args = parser.parse_args()
    if os.name != 'nt' or args.jobs < 1:
        parser.error('Run on Windows with --jobs >= 1.')
    msys = args.msys_root.resolve()
    for name in ['usr/bin/bash.exe', 'usr/bin/make.exe', 'usr/bin/gpg.exe', 'mingw64/bin/gcc.exe', 'mingw64/bin/strip.exe']:
        if not (msys / name).is_file():
            parser.error('Missing MSYS2 build tool: ' + str(msys / name))
    work = args.work_directory.resolve()
    work.mkdir(parents=True, exist_ok=True)
    source_url = f'https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz'
    archive = work / f'ffmpeg-{VERSION}.tar.xz'
    signature = work / (archive.name + '.asc')
    key = work / 'ffmpeg-devel.asc'
    download(source_url, archive, SOURCE_SHA256)
    download(source_url + '.asc', signature)
    download('https://ffmpeg.org/ffmpeg-devel.asc', key)
    home = work / 'gnupg'
    home.mkdir(exist_ok=True)
    env = os.environ | {'PATH': str(msys / 'mingw64/bin') + os.pathsep + str(msys / 'usr/bin') + os.pathsep + os.environ['PATH']}
    gpg = [str(msys / 'usr/bin/gpg.exe'), '--no-options', '--batch', '--no-autostart', '--homedir', posix(home)]
    subprocess.run(gpg + ['--import', posix(key)], env=env, check=True, timeout=45)
    verified = subprocess.run(gpg + ['--status-fd', '1', '--verify', posix(signature), posix(archive)],
                              env=env, check=True, timeout=45, capture_output=True)
    status = verified.stdout.decode('utf-8', 'replace')
    if f'[GNUPG:] VALIDSIG {SIGNER} ' not in status:
        raise ValueError('Unexpected FFmpeg release signing key.')
    (work / 'signature-verification.txt').write_bytes(verified.stdout + verified.stderr)
    source = work / f'ffmpeg-{VERSION}'
    if not source.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(work, filter='data')
    else:
        # A pinned archive must not silently bless a modified extracted tree.
        with tarfile.open(archive) as tar:
            for entry in tar:
                if entry.isfile():
                    path = work / entry.name
                    original = tar.extractfile(entry)
                    if not path.is_file() or sha(path) != hashlib.file_digest(original, 'sha256').hexdigest():
                        raise ValueError('Extracted source changed; use a fresh --work-directory: ' + str(path))
    nasm_url = f'https://www.nasm.us/pub/nasm/releasebuilds/{NASM_VERSION}/win64/nasm-{NASM_VERSION}-win64.zip'
    nasm_archive = work / f'nasm-{NASM_VERSION}-win64.zip'
    download(nasm_url, nasm_archive, NASM_SHA256)
    with zipfile.ZipFile(nasm_archive) as zip_file:
        for name in zip_file.namelist():
            if Path(name).is_absolute() or '..' in Path(name).parts:
                raise ValueError('Unsafe NASM archive entry: ' + name)
        zip_file.extractall(work / 'nasm')
    nasm = work / 'nasm' / f'nasm-{NASM_VERSION}' / 'nasm.exe'
    build = work / 'compile'
    prefix = work / 'install'
    build.mkdir(exist_ok=True)
    prefix.mkdir(exist_ok=True)
    configure = [short_path(source) + '/configure', '--prefix=' + short_path(prefix),
        '--target-os=mingw32', '--arch=x86_64', '--cpu=x86-64', '--cc=gcc', '--cxx=g++',
        '--enable-shared', '--disable-static', '--disable-programs', '--disable-doc', '--disable-debug',
        '--disable-autodetect', '--disable-everything', '--disable-network',
        '--disable-avdevice', '--disable-avfilter', '--disable-swscale', '--disable-swresample',
        '--enable-avcodec', '--enable-avformat', '--enable-avutil',
        '--enable-decoder=mlp', '--enable-encoder=mlp', '--enable-parser=mlp',
        '--enable-demuxer=mlp', '--enable-muxer=mlp', '--enable-protocol=file,pipe',
        '--disable-pthreads', '--enable-w32threads', '--enable-small', '--enable-gpl', '--enable-version3',
        '--x86asmexe=' + short_path(nasm), '--extra-cflags=-ffunction-sections -fdata-sections',
        '--extra-ldflags=-Wl,--gc-sections -Wl,--no-insert-timestamp -static-libgcc']
    if args.profile == 'media':
        configure = [item for item in configure if not item.startswith(('--enable-decoder=', '--enable-encoder=',
                     '--enable-parser=', '--enable-demuxer=', '--enable-muxer='))
                     and item not in ('--disable-swscale', '--disable-swresample')]
        configure += ['--enable-swscale', '--enable-swresample', '--enable-libsoxr', '--enable-zlib',
            '--enable-decoder=mlp,flac,alac,aac,pcm_s8,pcm_u8,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,mpeg2video,png,mjpeg',
            '--enable-encoder=mlp,flac,pcm_s16le,pcm_s24le,png',
            '--enable-parser=mlp,flac,aac,mpegvideo,png,mjpeg',
            '--enable-demuxer=mlp,flac,mov,wav,mpegps,mpegvideo', '--enable-muxer=mlp,flac,wav']
    commands = ['set -eu', 'export PATH=/mingw64/bin:/usr/bin', 'export LC_ALL=C',
                'export SOURCE_DATE_EPOCH=1789699562', 'cd ' + shlex.quote(short_path(build)),
                shlex.join(configure), f'make -j{args.jobs}', 'make install']
    script = work / 'build.sh'
    script.write_text('\n'.join(commands) + '\n', encoding='utf-8', newline='\n')
    print('Building native Windows x64 DLLs; log: ' + str(work / 'build.log'), flush=True)
    with (work / 'build.log').open('wb') as log:
        subprocess.run([str(msys / 'usr/bin/bash.exe'), '--noprofile', '--norc', posix(script)],
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    actual = {p.name for p in (prefix / 'bin').glob('*.dll')}
    expected_dlls = set(DLL_NAMES) | ({'swscale-10.dll', 'swresample-7.dll'} if args.profile == 'media' else set())
    if actual != expected_dlls:
        raise ValueError('Unexpected installed DLL set: ' + repr(actual))
    compiler = subprocess.check_output([str(msys / 'mingw64/bin/gcc.exe'), '--version'], env=env).decode().splitlines()[0]
    record = {'version': VERSION, 'source_url': source_url, 'source_sha256': SOURCE_SHA256,
              'signer': SIGNER, 'nasm_url': nasm_url, 'nasm_sha256': NASM_SHA256,
              'compiler': compiler, 'configure': configure, 'target': 'Windows x64',
              'files': {name: {'sha256': sha(prefix / 'bin' / name), 'bytes': (prefix / 'bin' / name).stat().st_size}
                        for name in sorted(expected_dlls)}}
    record['profile'] = args.profile
    (prefix / 'build-manifest.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(record, indent=2), flush=True)


if __name__ == '__main__':
    main()
