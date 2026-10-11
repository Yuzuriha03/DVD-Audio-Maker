"""Build the retained FFmpeg profiles and their dependencies with MSVC x64.

MSYS2 supplies only the POSIX configure/make shell. C compilation, archive
creation, and DLL linking use Visual Studio. zlib and libsoxr are linked
statically into the five FFmpeg DLLs; no MinGW runtime is included.
"""
from pathlib import Path
import argparse
import ctypes
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

from msvc_env import msvc_environment
from native.pe_dependencies import Pe
from worker_policy import worker_count

TARGET = 'x86_64-pc-windows-msvc'
VERSION = '9.0.2'
SOURCE_SHA256 = '8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'
SIGNER = 'FCF986EA15E6E293A5644F10B4322F04D67658D8'
NASM_VERSION = '2.16.03'
NASM_SHA256 = '3ee4782247bcb874378d02f7eab4e294a84d3d15f3f6ee2de2f47a46aa7226e6'
SOXR_URL = 'https://codeload.github.com/chirlu/soxr/tar.gz/refs/tags/0.1.3'
SOXR_SHA256 = 'db6ca1b1e8405c6ef92f8294fc123d910abf0a114003b3f0f13fa57a95fd62d0'
ZLIB_URL = 'https://zlib.net/fossils/zlib-1.3.2.tar.gz'
ZLIB_SHA256 = 'bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16'


def sha(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def posix(path):
    value = Path(path).absolute().as_posix()
    if len(value) < 3 or value[1:3] != ':/':
        raise ValueError('An absolute Windows drive path is required: '+value)
    return '/'+value[0].lower()+value[2:]


def short_path(path):
    buffer = ctypes.create_unicode_buffer(32768)
    call = ctypes.windll.kernel32.GetShortPathNameW
    call.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint]
    call.restype = ctypes.c_uint
    size = call(str(Path(path).resolve()), buffer, len(buffer))
    if not size or size >= len(buffer) or any(c.isspace() for c in buffer.value):
        raise OSError('Use a build directory with Windows short names or without spaces: '+str(path))
    return posix(buffer.value)


def native_short_path(path):
    value = short_path(path)
    return value[1].upper()+':'+value[2:]


def download(url, target, expected=None, cache=None):
    if not target.exists():
        cached = cache/target.name if cache else None
        if cached and cached.is_file():
            shutil.copyfile(cached, target)
        else:
            print('Downloading '+url, flush=True)
            request = urllib.request.Request(url, headers={'User-Agent': 'DVD-Audio-Maker-native-build'})
            with urllib.request.urlopen(request, timeout=90) as response:
                data = response.read()
            if expected and hashlib.sha256(data).hexdigest() != expected:
                raise ValueError('Downloaded archive hash mismatch: '+url)
            target.write_bytes(data)
    if expected and sha(target) != expected:
        raise ValueError('Cached archive hash mismatch: '+str(target))


def extract(archive, parent, dirname, patches=None):
    source = parent/dirname
    patches = patches or {}
    with tarfile.open(archive) as tar:
        if not source.is_dir():
            tar.extractall(parent, filter='data')
        else:
            for member in tar:
                if member.isfile():
                    original = tar.extractfile(member).read()
                    relative = Path(member.name).relative_to(dirname).as_posix()
                    if relative in patches:
                        old, new = patches[relative]
                        if old not in original:
                            raise ValueError('Expected source patch not found: '+relative)
                        original = original.replace(old, new)
                    path = parent/member.name
                    if not path.is_file() or sha(path) != hashlib.sha256(original).hexdigest():
                        raise ValueError('Extracted source changed; use a fresh build directory: '+str(path))
    for relative, (old, new) in patches.items():
        path = source/relative
        data = path.read_bytes()
        if old in data:
            path.write_bytes(data.replace(old, new))
    return source


def run_logged(command, log, env, cwd=None):
    subprocess.run([str(item) for item in command], cwd=cwd, env=env,
                   stdout=log, stderr=subprocess.STDOUT, check=True)


def build_dependencies(work, downloads, cache, cmake, env, log):
    sources = work/'sources'
    sources.mkdir(exist_ok=True)
    prefix = work/'dependencies'
    for directory in ('include', 'lib'):
        (prefix/directory).mkdir(parents=True, exist_ok=True)
    soxr_archive = downloads/'soxr-archive.tar.gz'
    download(SOXR_URL, soxr_archive, SOXR_SHA256, cache)
    # Upstream 0.1.3 leaves this CMake path unquoted. Windows workspaces with
    # spaces need the quoted condition; no library source or engine changes.
    soxr_patches = {'CMakeLists.txt': (
        b'EXISTS ${PROJECT_SOURCE_DIR}/lsr-tests;',
        b'EXISTS \\"${PROJECT_SOURCE_DIR}/lsr-tests\\";')}
    soxr = sources/'soxr-0.1.3'
    if soxr.is_dir():
        path = soxr/'CMakeLists.txt'
        old, new = soxr_patches['CMakeLists.txt']
        if old in path.read_bytes():
            path.write_bytes(path.read_bytes().replace(old, new))
    soxr = extract(soxr_archive, sources, 'soxr-0.1.3', soxr_patches)
    soxr_build = work/'soxr-compile-msvc'
    soxr_build.mkdir(exist_ok=True)
    run_logged([cmake, '-S', native_short_path(soxr), '-B', native_short_path(soxr_build), '-G', 'NMake Makefiles',
                '-DCMAKE_BUILD_TYPE=Release', '-DBUILD_SHARED_LIBS=OFF',
                '-DBUILD_TESTS=ON', '-DBUILD_EXAMPLES=OFF', '-DWITH_OPENMP=OFF',
                '-DWITH_LSR_BINDINGS=OFF', '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW',
                '-DCMAKE_POLICY_DEFAULT_CMP0127=NEW',
                '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL',
                '-DCMAKE_INSTALL_PREFIX='+native_short_path(prefix)], log, env)
    run_logged([cmake, '--build', soxr_build], log, env)
    # Native libsoxr sanity tests exercise the compiled filter engines.
    ctest = Path(cmake).with_name('ctest.exe')
    run_logged([ctest, '--test-dir', soxr_build, '--output-on-failure'], log, env)
    run_logged([cmake, '--install', soxr_build], log, env)
    zlib_archive = downloads/'zlib-1.3.2.tar.gz'
    download(ZLIB_URL, zlib_archive, ZLIB_SHA256, cache)
    zlib = extract(zlib_archive, sources, 'zlib-1.3.2')
    zlib_build = work/'zlib-compile'
    zlib_build.mkdir(exist_ok=True)
    zlib_sources = ('adler32', 'compress', 'crc32', 'deflate', 'gzclose', 'gzlib',
                    'gzread', 'gzwrite', 'inflate', 'infback', 'inftrees',
                    'inffast', 'trees', 'uncompr', 'zutil')
    run_logged([env['CC'], '/nologo', '/c', '/O2', '/MD', '/std:c17',
                '/D_CRT_SECURE_NO_WARNINGS', '/I'+str(zlib)] +
               [zlib/(name+'.c') for name in zlib_sources], log, env, zlib_build)
    run_logged([env['DVDA_LIB_TOOL'], '/nologo', '/machine:x64',
                '/out:'+str(prefix/'lib/zlib.lib')] +
               [zlib_build/(name+'.obj') for name in zlib_sources], log, env)
    for name in ('zlib.h', 'zconf.h'):
        shutil.copyfile(zlib/name, prefix/'include'/name)
    run_logged([env['CC'], '/nologo', '/O2', '/MD', '/I'+str(zlib),
                zlib/'test/example.c', prefix/'lib/zlib.lib',
                '/Fe:'+str(zlib_build/'zlib-test.exe'), '/Fo:'+str(zlib_build/'zlib-test.obj')], log, env)
    run_logged([zlib_build/'zlib-test.exe'], log, env, zlib_build)
    return prefix


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', type=Path, default=Path(os.environ.get('MSYS2_ROOT', 'C:/msys64')))
    parser.add_argument('--work-directory', type=Path, default=repo/'build/msvc-native/ffmpeg')
    parser.add_argument('--source-cache', type=Path, help='Optional directory holding pinned FFmpeg/NASM/source archives')
    parser.add_argument('--cmake', type=Path, help='Native Windows CMake executable for MSVC libsoxr')
    parser.add_argument('--profile', choices=['mlp', 'menu', 'media', 'shared'], default='shared')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Run on Windows with the Visual Studio C++ x64 build tools.')
    env = msvc_environment()
    msys = args.msys_root.resolve()
    for name in ('bash.exe', 'make.exe', 'gpg.exe'):
        if not (msys/'usr/bin'/name).is_file():
            parser.error('Missing POSIX maintenance tool: '+str(msys/'usr/bin'/name))
    work = args.work_directory.resolve()
    work.mkdir(parents=True, exist_ok=True)
    cmake = args.cmake or shutil.which('cmake.exe', path=env['PATH'])
    if not cmake:
        candidate = work/'build-tools/cmake/data/bin/cmake.exe'
        if candidate.is_file():
            cmake = candidate
    if not cmake or not Path(cmake).is_file():
        parser.error('Native Windows CMake is required; supply --cmake (no MinGW CMake).')
    downloads = work/'downloads'
    downloads.mkdir(exist_ok=True)
    cache = args.source_cache.resolve() if args.source_cache else None
    source_url = f'https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz'
    archive = downloads/f'ffmpeg-{VERSION}.tar.xz'
    signature = downloads/(archive.name+'.asc')
    key = downloads/'ffmpeg-devel.asc'
    download(source_url, archive, SOURCE_SHA256, cache)
    download(source_url+'.asc', signature, cache=cache)
    download('https://ffmpeg.org/ffmpeg-devel.asc', key, cache=cache)
    gnupg = work/'gnupg'
    gnupg.mkdir(exist_ok=True)
    env['PATH'] += os.pathsep+str(msys/'usr/bin')
    gpg = [str(msys/'usr/bin/gpg.exe'), '--no-options', '--batch', '--no-autostart', '--homedir', posix(gnupg)]
    subprocess.run(gpg+['--import', posix(key)], env=env, check=True, timeout=45)
    verified = subprocess.run(gpg+['--status-fd', '1', '--verify', posix(signature), posix(archive)],
                              env=env, check=True, timeout=45, capture_output=True)
    if f'[GNUPG:] VALIDSIG {SIGNER} ' not in verified.stdout.decode('utf-8', 'replace'):
        raise ValueError('Unexpected FFmpeg release signing key.')
    (work/'signature-verification.txt').write_bytes(verified.stdout+verified.stderr)
    sources = work/'sources'
    sources.mkdir(exist_ok=True)
    source = extract(archive, sources, f'ffmpeg-{VERSION}')
    nasm_url = f'https://www.nasm.us/pub/nasm/releasebuilds/{NASM_VERSION}/win64/nasm-{NASM_VERSION}-win64.zip'
    nasm_archive = downloads/f'nasm-{NASM_VERSION}-win64.zip'
    download(nasm_url, nasm_archive, NASM_SHA256, cache)
    with zipfile.ZipFile(nasm_archive) as file:
        for name in file.namelist():
            if Path(name).is_absolute() or '..' in Path(name).parts:
                raise ValueError('Unsafe NASM archive entry: '+name)
        file.extractall(work/'nasm')
    nasm = work/'nasm'/f'nasm-{NASM_VERSION}'/'nasm.exe'
    build, prefix = work/'compile', work/'install'
    build.mkdir(exist_ok=True)
    prefix.mkdir(exist_ok=True)
    print('MSVC source build log: '+str(work/'build.log'), flush=True)
    with (work/'build.log').open('wb') as log:
        deps = build_dependencies(work, downloads, cache, Path(cmake).resolve(), env, log)
        configure = [short_path(source)+'/configure', '--prefix='+short_path(prefix),
                     '--toolchain=msvc', '--target-os=win32', '--arch=x86_64', '--cpu=x86-64',
                     '--enable-shared', '--disable-static', '--disable-programs', '--disable-doc', '--disable-debug',
                     '--disable-autodetect', '--disable-everything', '--disable-network',
                     '--disable-avdevice', '--disable-avfilter', '--disable-swscale', '--disable-swresample',
                     '--enable-avcodec', '--enable-avformat', '--enable-avutil',
                     '--enable-decoder=mlp', '--enable-encoder=mlp', '--enable-parser=mlp',
                     '--enable-demuxer=mlp', '--enable-muxer=mlp', '--enable-protocol=file,pipe',
                     '--disable-pthreads', '--enable-w32threads', '--enable-small', '--enable-gpl', '--enable-version3',
                     '--x86asmexe='+short_path(nasm),
                     '--extra-cflags=-MD -I'+native_short_path(deps/'include'),
                     '--extra-ldflags=-libpath:'+native_short_path(deps/'lib')]
        if args.profile in ('menu', 'media', 'shared'):
            configure = [item for item in configure if not item.startswith(('--enable-decoder=', '--enable-encoder=',
                         '--enable-parser=', '--enable-demuxer=', '--enable-muxer='))
                         and item not in ('--disable-swscale', '--disable-swresample')]
        if args.profile == 'menu':
            configure += ['--disable-swscale', '--disable-swresample',
                          '--enable-decoder=mlp,pcm_s16le,pcm_s24le,pcm_s32le', '--enable-encoder=mlp,mpeg2video,mp2',
                          '--enable-parser=mlp,mpegvideo,mpegaudio', '--enable-demuxer=mlp,wav', '--enable-muxer=mlp,mpeg2dvd']
        if args.profile in ('media', 'shared'):
            configure += ['--enable-swscale', '--enable-swresample', '--enable-libsoxr', '--enable-zlib',
                '--enable-decoder=mlp,flac,alac,aac,pcm_s8,pcm_u8,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,mpeg2video,png,mjpeg',
                '--enable-encoder=mlp,flac,pcm_s16le,pcm_s24le,png', '--enable-parser=mlp,flac,aac,mpegvideo,png,mjpeg',
                '--enable-demuxer=mlp,flac,mov,wav,mpegps,mpegvideo', '--enable-muxer=mlp,flac,wav']
        if args.profile == 'shared':
            configure += ['--enable-encoder=mpeg2video,mp2', '--enable-parser=mpegaudio', '--enable-muxer=mpeg2dvd']
        commands = ['set -eu', 'export LC_ALL=C', 'export SOURCE_DATE_EPOCH=1789699562',
                    'cd '+shlex.quote(short_path(build)), shlex.join(configure),
                    f'make -j{worker_count()}', 'make install']
        script = work/'build.sh'
        script.write_text('\n'.join(commands)+'\n', encoding='utf-8', newline='\n')
        run_logged([msys/'usr/bin/bash.exe', '--noprofile', '--norc', posix(script)], log, env)
    expected = {'avcodec-63.dll', 'avformat-63.dll', 'avutil-61.dll'}
    if args.profile in ('media', 'shared'):
        expected |= {'swscale-10.dll', 'swresample-7.dll'}
    actual = {path.name for path in (prefix/'bin').glob('*.dll')}
    if actual != expected:
        raise ValueError('Unexpected FFmpeg DLL set: '+repr(actual))
    imports = {}
    for name in sorted(expected):
        pe = Pe(prefix/'bin'/name)
        if pe.machine != 0x8664:
            raise ValueError('FFmpeg DLL is not x64: '+name)
        imports[name] = sorted(pe.imports())
        if any(item.startswith(('libgcc', 'libstdc++', 'libwinpthread', 'msys-')) for item in imports[name]):
            raise ValueError('GNU runtime dependency in MSVC FFmpeg DLL: '+name)
    # Save .def files consumed by the Rust layout/link build script.
    for path in build.glob('lib*/*.def'):
        shutil.copyfile(path, prefix/'lib'/path.name)
    record = {'version': VERSION, 'profile': args.profile, 'target': TARGET, 'toolchain': 'msvc',
              'source_url': source_url, 'source_sha256': SOURCE_SHA256, 'signer': SIGNER,
              'nasm_url': nasm_url, 'nasm_sha256': NASM_SHA256,
              'compiler': env['CC'], 'configure': configure,
              'imports': imports,
              'dependencies': {'soxr': {'version': '0.1.3', 'source_url': SOXR_URL, 'source_sha256': SOXR_SHA256,
                                         'linkage': 'static', 'toolchain': 'msvc',
                                         'source_patch': 'Quote CMake lsr-tests path for workspaces containing spaces',
                                         'validation': '9 upstream native resampler tests'},
                               'zlib': {'version': '1.3.2', 'source_url': ZLIB_URL, 'source_sha256': ZLIB_SHA256,
                                        'linkage': 'static', 'toolchain': 'msvc',
                                        'validation': 'Native upstream compression/decompression/gzip/dictionary example'}},
              'files': {name: {'sha256': sha(prefix/'bin'/name), 'bytes': (prefix/'bin'/name).stat().st_size}
                        for name in sorted(expected)}}
    (prefix/'build-manifest.json').write_text(json.dumps(record, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(record, indent=2), flush=True)


if __name__ == '__main__':
    main()
